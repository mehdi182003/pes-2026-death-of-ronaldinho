//! Live match state read from PES's memory: the ball (documented pointer)
//! and the players (found by scanning, see `pes::find_runs`).

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};

use crate::memory;
use crate::pes::{self, DistanceForm, Run};
use crate::proxy::log;

/// Position of the ball, if PES has one in memory right now.
pub fn ball_position() -> Option<[f32; 3]> {
    let pointer = memory::read::<u32>(memory::exe_base() + pes::BALL_GLOBAL_PTR_RVA)? as usize;
    let position = memory::read::<[f32; 3]>(pointer + pes::BALL_POSITION)?;
    pes::plausible_position(position).then_some(position)
}

/// Positions of the structs of the best run found by the scan.
pub fn player_positions() -> Option<Vec<[f32; 3]>> {
    let run = (*PLAYER_RUN.lock().ok()?)?;
    Some(
        (0..run.count)
            .map(|i| {
                memory::read::<[f32; 3]>(run.start + i * run.stride + pes::PLAYER_POSITION)
                    .unwrap_or([f32::NAN; 3])
            })
            .collect(),
    )
}

static PLAYER_RUN: Mutex<Option<Run>> = Mutex::new(None);
static SCAN_RUNNING: AtomicBool = AtomicBool::new(false);
/// Frames in a match with a readable ball since the last scan.
static MATCH_FRAMES: AtomicU32 = AtomicU32::new(0);
static SCANS: AtomicU32 = AtomicU32::new(0);

/// Frames to wait (about 10 s) so the match is set up before scanning, and
/// between two attempts.
const FRAMES_BEFORE_SCAN: u32 = 600;
/// Attempts before giving up for this session.
const MAX_SCANS: u32 = 3;
/// Largest spacing between two player structs considered.
const MAX_STRIDE: usize = 0x4000;
/// A run needs at least one team.
const MIN_RUN: usize = 11;
/// Bytes copied at once while scanning.
const CHUNK: usize = 1 << 20;
/// Stops collecting signed candidates beyond this.
const MAX_CANDIDATES: usize = 100_000;

/// Called once per frame in a match: starts the player scan in the
/// background after the match has run for a while, and retries if it found
/// nothing.
pub fn on_match_frame() {
    let found = PLAYER_RUN.lock().map(|run| run.is_some()).unwrap_or(true);
    if found
        || SCAN_RUNNING.load(Relaxed)
        || SCANS.load(Relaxed) >= MAX_SCANS
        || ball_position().is_none()
    {
        return;
    }
    if MATCH_FRAMES.fetch_add(1, Relaxed) + 1 >= FRAMES_BEFORE_SCAN
        && !SCAN_RUNNING.swap(true, Relaxed)
    {
        MATCH_FRAMES.store(0, Relaxed);
        SCANS.fetch_add(1, Relaxed);
        std::thread::spawn(|| {
            scan_players();
            SCAN_RUNNING.store(false, Relaxed);
        });
    }
}

fn scan_players() {
    let started = std::time::Instant::now();
    let regions: Vec<_> = memory::readable_regions()
        .into_iter()
        .filter(|r| r.writable)
        .collect();
    let total: usize = regions.iter().map(|r| r.size).sum();
    log(&format!(
        "recherche des joueurs (essai {}) : {} zones inscriptibles, {} Mo",
        SCANS.load(Relaxed),
        regions.len(),
        total >> 20
    ));

    // Shape alone (a position, a pointer) matches hundreds of thousands of
    // places; the stored distance to the ball is the real signature.
    let mut shaped = 0usize;
    let mut forms: BTreeMap<DistanceForm, usize> = BTreeMap::new();
    let mut candidates = Vec::new();
    'regions: for region in &regions {
        let end = region.base + region.size;
        let mut at = region.base;
        while at < end {
            let len = (end - at).min(CHUNK + pes::PLAYER_PROBE_LEN);
            let Some(bytes) = memory::read_bytes(at, len) else {
                break;
            };
            let Some(ball) = ball_position() else {
                log("ballon perdu pendant la recherche : arrêt");
                return;
            };
            let last = bytes.len().saturating_sub(pes::PLAYER_PROBE_LEN);
            for offset in (0..=last.min(CHUNK - 4)).step_by(4) {
                let probe = &bytes[offset..];
                if !pes::looks_like_player(probe) {
                    continue;
                }
                shaped += 1;
                let Some(form) = pes::ball_distance_form(probe, ball) else {
                    continue;
                };
                *forms.entry(form).or_default() += 1;
                candidates.push(at + offset);
                if candidates.len() >= MAX_CANDIDATES {
                    log("trop de candidats : recherche arrêtée");
                    break 'regions;
                }
            }
            at += CHUNK;
        }
    }
    candidates.sort_unstable();
    let runs = pes::find_runs(&candidates, MAX_STRIDE, MIN_RUN);
    log(&format!(
        "{shaped} structures de la bonne forme, {} avec la distance au ballon ({forms:?}), {} suites régulières, en {:.1} s",
        candidates.len(),
        runs.len(),
        started.elapsed().as_secs_f64()
    ));
    for run in runs.iter().take(8) {
        log(&format!(
            "  suite à {:#010x} : {} structures tous les {:#x} octets",
            run.start, run.count, run.stride
        ));
    }
    if runs.is_empty() {
        for address in candidates.iter().take(40) {
            let position = memory::read::<[f32; 3]>(address + pes::PLAYER_POSITION);
            log(&format!("  candidat isolé {address:#010x} : {position:?}"));
        }
        return;
    }
    let best = runs[0];
    for i in 0..best.count.min(32) {
        let position =
            memory::read::<[f32; 3]>(best.start + i * best.stride + pes::PLAYER_POSITION);
        log(&format!("    n° {i:2} : {position:?}"));
    }
    if let Ok(mut run) = PLAYER_RUN.lock() {
        *run = Some(best);
    }
}
