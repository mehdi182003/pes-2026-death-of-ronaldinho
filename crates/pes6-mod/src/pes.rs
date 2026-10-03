//! What is known of PES 6's memory (retail Win32 PES6.exe, the build of
//! Mehdi's install). Every layout comes from the decompiled code (Ghidra,
//! project `~/pes6-decomp`), cited by function; see `docs/pes6-mod.md`.
//!
//! The ball pointer was first published by the open-source PES 6 physics
//! mod by angelballay (https://mintlify.wiki/angelballay/pes6_game_physics_mod),
//! and is used the same way by the decompiled code below.

/// Base address PES6.exe is linked at (no relocations: it always loads there).
pub const IMAGE_BASE: usize = 0x40_0000;

/// RVA of the global pointer to the ball struct (`PTR_DAT_00bcce94` in
/// FUN_005a2a7b and FUN_00478020).
pub const BALL_GLOBAL_PTR_RVA: usize = 0x7C_CE94;
/// Ball struct: world position, three floats (X, Y = height, Z).
pub const BALL_POSITION: usize = 0x20;

/// The player array (`DAT_03bdc980`): FUN_005a2a7b takes player `i` at
/// `0x03bdc980 + i * 0x240`, with `i` modulo `0x17`.
pub const PLAYERS_RVA: usize = 0x03BD_C980 - IMAGE_BASE;
pub const PLAYER_STRIDE: usize = 0x240;
pub const PLAYER_SLOTS: usize = 0x17;
/// Player struct: identifier byte (`*param_1` in FUN_00478020).
pub const PLAYER_ID: usize = 0x00;
/// Player struct: number within the team, indexes the stats table
/// `0x03bcf5a8 + (team * 0x20 + number) * 0x348` (FUN_005a2a7b).
pub const PLAYER_NUMBER: usize = 0x11;
/// Player struct: team, 0 or 1 (`param_1[0x12]` and its `^ 1` in FUN_00478020).
pub const PLAYER_TEAM: usize = 0x12;
/// Player struct: position, three floats X, Y, Z (copied by FUN_00478020;
/// X and Z read as `DAT_03bdca60` / `DAT_03bdca68` in FUN_005a2a7b).
pub const PLAYER_POSITION: usize = 0xE0;
/// Bytes of a player struct read by [`Player::parse`].
pub const PLAYER_READ_LEN: usize = PLAYER_POSITION + 12;

/// Logic coordinates (ball and players in memory): 5 times the render
/// world's 51.3 units per metre (M3 log: the ball at rest is at Y = −28,
/// i.e. 11 cm above the grass, its radius). Y points down, like the render
/// world.
pub const LOGIC_UNITS_PER_METRE: f32 = 256.5;

// HYPOTHÈSE: bounds of a logic position on or near the pitch: a 105 m × 68 m
// pitch is about ±13 500 × ±8 700 units; the margin covers the run-off area.
pub const PITCH_HALF_EXTENT: f32 = 18_000.0;
/// Highest ball, about 40 m.
pub const MAX_HEIGHT: f32 = 10_000.0;

/// True when the three floats can be a position on or above the pitch.
pub fn plausible_position([x, y, z]: [f32; 3]) -> bool {
    [x, y, z].iter().all(|v| v.is_finite())
        && x.abs() <= PITCH_HALF_EXTENT
        && z.abs() <= PITCH_HALF_EXTENT
        && y.abs() <= MAX_HEIGHT
}

/// RVA of player slot `index`.
pub fn player_rva(index: usize) -> usize {
    PLAYERS_RVA + index * PLAYER_STRIDE
}

/// The fields of a player struct the mod uses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Player {
    pub slot: usize,
    pub id: u8,
    pub team: u8,
    pub number: u8,
    pub position: [f32; 3],
}

impl Player {
    /// Reads the first [`PLAYER_READ_LEN`] bytes of slot `slot`.
    pub fn parse(slot: usize, bytes: &[u8]) -> Option<Self> {
        let bytes = bytes.get(..PLAYER_READ_LEN)?;
        let f = |at: usize| f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        Some(Self {
            slot,
            id: bytes[PLAYER_ID],
            team: bytes[PLAYER_TEAM],
            number: bytes[PLAYER_NUMBER],
            position: [
                f(PLAYER_POSITION),
                f(PLAYER_POSITION + 4),
                f(PLAYER_POSITION + 8),
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_match_the_decompiled_addresses() {
        assert_eq!(IMAGE_BASE + player_rva(0), 0x03BD_C980);
        // The default player of FUN_005a2a7b, `&DAT_03bdcbc0`, is slot 1.
        assert_eq!(IMAGE_BASE + player_rva(1), 0x03BD_CBC0);
        // X and Z of slot 0 are DAT_03bdca60 and DAT_03bdca68.
        assert_eq!(IMAGE_BASE + player_rva(0) + PLAYER_POSITION, 0x03BD_CA60);
        assert_eq!(
            IMAGE_BASE + player_rva(0) + PLAYER_POSITION + 8,
            0x03BD_CA68
        );
    }

    #[test]
    fn parses_a_player() {
        let mut bytes = vec![0u8; PLAYER_READ_LEN];
        bytes[PLAYER_ID] = 7;
        bytes[PLAYER_NUMBER] = 9;
        bytes[PLAYER_TEAM] = 1;
        for (i, v) in [1200.0f32, 0.0, -800.0].iter().enumerate() {
            bytes[PLAYER_POSITION + 4 * i..PLAYER_POSITION + 4 * i + 4]
                .copy_from_slice(&v.to_le_bytes());
        }
        let p = Player::parse(3, &bytes).unwrap();
        assert_eq!((p.slot, p.id, p.number, p.team), (3, 7, 9, 1));
        assert_eq!(p.position, [1200.0, 0.0, -800.0]);
        assert!(Player::parse(3, &bytes[..10]).is_none());
    }

    #[test]
    fn positions_on_the_pitch_are_plausible() {
        assert!(plausible_position([3458.4, -28.0, 4051.2]));
        assert!(!plausible_position([1e9, 0.0, 0.0]));
        assert!(!plausible_position([f32::NAN, 0.0, 0.0]));
    }
}
