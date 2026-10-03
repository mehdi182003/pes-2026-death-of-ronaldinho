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
