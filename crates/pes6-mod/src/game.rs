//! Live match state read from PES's memory: the ball (documented pointer)
//! and the players (found by scanning, see `pes::find_runs`).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};

use crate::memory;
use crate::pes::{self, Run};
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
static SCAN_STARTED: AtomicBool = AtomicBool::new(false);
/// Frames in a match with a readable ball, before the scan starts.
static MATCH_FRAMES: AtomicU32 = AtomicU32::new(0);

/// Frames to wait (about 10 s) so the match is set up before scanning.
const FRAMES_BEFORE_SCAN: u32 = 600;
/// Largest spacing between two player structs considered.
const MAX_STRIDE: usize = 0x4000;
/// A run needs at least one team.
const MIN_RUN: usize = 11;
/// Bytes copied at once while scanning.
const CHUNK: usize = 1 << 20;
/// Stops collecting candidates beyond this (memory full of plausible floats).
const MAX_CANDIDATES: usize = 400_000;

/// Called once per frame in a match: starts the player scan in the
/// background after the match has run for a while.
pub fn on_match_frame() {
    if ball_position().is_none() || SCAN_STARTED.load(Relaxed) {
        return;
    }
    if MATCH_FRAMES.fetch_add(1, Relaxed) + 1 >= FRAMES_BEFORE_SCAN
        && !SCAN_STARTED.swap(true, Relaxed)
    {
        std::thread::spawn(scan_players);
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
        "recherche des joueurs : {} zones inscriptibles, {} Mo",
        regions.len(),
        total >> 20
    ));

    let mut candidates = Vec::new();
    'regions: for region in &regions {
        let end = region.base + region.size;
        let mut at = region.base;
        while at < end {
            let len = (end - at).min(CHUNK + pes::PLAYER_PROBE_LEN);
            let Some(bytes) = memory::read_bytes(at, len) else {
                break;
            };
            let last = bytes.len().saturating_sub(pes::PLAYER_PROBE_LEN);
            for offset in (0..=last.min(CHUNK - 4)).step_by(4) {
                if pes::looks_like_player(&bytes[offset..]) {
                    let physics = u32::from_le_bytes(
                        bytes[offset + pes::PLAYER_PHYSICS_PTR
                            ..offset + pes::PLAYER_PHYSICS_PTR + 4]
                            .try_into()
                            .unwrap(),
                    ) as usize;
                    if memory::readable(physics, 4) {
                        candidates.push(at + offset);
                        if candidates.len() >= MAX_CANDIDATES {
                            log("trop de candidats : recherche arrêtée");
                            break 'regions;
                        }
                    }
                }
            }
            at += CHUNK;
        }
    }
    candidates.sort_unstable();
    let runs = pes::find_runs(&candidates, MAX_STRIDE, MIN_RUN);
    log(&format!(
        "{} candidats, {} suites régulières, en {:.1} s",
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
    if let Some(best) = runs.first() {
        for i in 0..best.count.min(32) {
            let position =
                memory::read::<[f32; 3]>(best.start + i * best.stride + pes::PLAYER_POSITION);
            log(&format!("    n° {i:2} : {position:?}"));
        }
        if let Ok(mut run) = PLAYER_RUN.lock() {
            *run = Some(*best);
        }
    }
}
