//! Live match state read from PES's memory, with the layouts of `pes`.

use crate::memory;
use crate::pes::{self, Player};

/// Position of the ball, if PES has one in memory right now.
pub fn ball_position() -> Option<[f32; 3]> {
    let pointer = memory::read::<u32>(memory::exe_base() + pes::BALL_GLOBAL_PTR_RVA)? as usize;
    let position = memory::read::<[f32; 3]>(pointer + pes::BALL_POSITION)?;
    pes::plausible_position(position).then_some(position)
}

/// Every slot of PES's player array.
pub fn players() -> Vec<Player> {
    let base = memory::exe_base();
    (0..pes::PLAYER_SLOTS)
        .filter_map(|slot| {
            let bytes = memory::read_bytes(base + pes::player_rva(slot), pes::PLAYER_READ_LEN)?;
            Player::parse(slot, &bytes)
        })
        .collect()
}

/// The recording being written (docs/BRIEF.md, R2).
struct Recorder {
    out: std::io::BufWriter<std::fs::File>,
    start: std::time::Instant,
    frames: u64,
}

static RECORDER: std::sync::Mutex<Option<Recorder>> = std::sync::Mutex::new(None);

/// Frames between two flushes of the recording (one second at 60 images/s).
const FLUSH_EVERY: u64 = 60;

/// Appends the ball and every player slot of this frame to the recording,
/// starting it on the first match frame.
pub fn record_frame(frame: u64) {
    use crate::record::{self, Record};
    use std::io::Write;

    let Ok(mut recorder) = RECORDER.lock() else {
        return;
    };
    if recorder.is_none() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let Ok(path) = std::env::current_exe()
            .map(|exe| exe.with_file_name(format!("chaos-fc-record-{stamp}.cfrc")))
        else {
            return;
        };
        let file = match std::fs::File::create(&path) {
            Ok(file) => file,
            Err(err) => {
                crate::proxy::log(&format!("enregistrement impossible : {err}"));
                return;
            }
        };
        let mut out = std::io::BufWriter::new(file);
        if record::write_header(&mut out, pes::PLAYER_SLOTS as u32).is_err() {
            return;
        }
        crate::proxy::log(&format!("enregistrement du match : {}", path.display()));
        *recorder = Some(Recorder {
            out,
            start: std::time::Instant::now(),
            frames: 0,
        });
    }
    let Some(rec) = recorder.as_mut() else {
        return;
    };
    let players = players();
    if players.len() != pes::PLAYER_SLOTS {
        return;
    }
    let record = Record {
        frame,
        seconds: rec.start.elapsed().as_secs_f64(),
        ball: ball_position(),
        players: players.iter().map(|p| p.position).collect(),
    };
    if record::write_record(&mut rec.out, &record).is_err() {
        return;
    }
    rec.frames += 1;
    if rec.frames.is_multiple_of(FLUSH_EVERY) {
        let _ = rec.out.flush();
    }
}
