//! What is known of PES 6's memory (retail Win32 PES6.exe, the build of
//! Mehdi's install), and the portable logic that searches it.
//!
//! Source of the ball and player offsets: the open-source PES 6 physics mod
//! by angelballay (https://mintlify.wiki/angelballay/pes6_game_physics_mod,
//! reference/pes-addresses). Checked on this PES6.exe in M3: the stock ball
//! mass 188.0 at 0xB8AE70, and its two documented call sites
//! (0x1A5905 → 0x1A1570, 0x1A6381 → 0x78020) are byte for byte there.

/// RVA of the global pointer to the ball struct.
pub const BALL_GLOBAL_PTR_RVA: usize = 0x7C_CE94;
/// Ball struct: world position, three floats (X, Y = height, Z).
pub const BALL_POSITION: usize = 0x20;

/// Player struct: "logic" position, three floats (X, Y, Z).
pub const PLAYER_POSITION: usize = 0xE0;
/// Player struct: pointer to the player's physics struct.
pub const PLAYER_PHYSICS_PTR: usize = 0xD0;
/// Player struct: discrete cell of the pitch grid (X, then Y/Z), one byte each.
pub const PLAYER_CELL: usize = 0x204;
/// Bytes of a player struct read by [`looks_like_player`].
pub const PLAYER_PROBE_LEN: usize = PLAYER_CELL + 2;

/// Players on the pitch in a match.
pub const PLAYERS: usize = 22;

// HYPOTHÈSE: bounds of a position on or near the pitch, in PES units. The
// marker of M2b shows the render world at ~51.3 units per metre (a 105 m ×
// 68 m pitch is about 5400 × 3500 units); the logic positions are assumed to
// use the same units, with a wide margin. To be confirmed with the ball post.
pub const PITCH_HALF_EXTENT: f32 = 6_000.0;
pub const MAX_HEIGHT: f32 = 2_000.0;

/// True when the three floats can be a position on or above the pitch.
pub fn plausible_position([x, y, z]: [f32; 3]) -> bool {
    [x, y, z].iter().all(|v| v.is_finite())
        && x.abs() <= PITCH_HALF_EXTENT
        && z.abs() <= PITCH_HALF_EXTENT
        && y.abs() <= MAX_HEIGHT
        // An all-zero block is far more often unused memory than a player
        // standing exactly on the centre spot.
        && (x, y, z) != (0.0, 0.0, 0.0)
}

fn f32_at(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

/// Position stored in a player struct.
pub fn player_position(bytes: &[u8]) -> [f32; 3] {
    [0, 4, 8].map(|o| f32_at(bytes, PLAYER_POSITION + o))
}

/// Cheap structural test of a candidate player struct (the first
/// [`PLAYER_PROBE_LEN`] bytes): a plausible position and a physics pointer
/// that looks like a user-mode address. The pointer itself is checked by the
/// caller, which can query the memory.
pub fn looks_like_player(bytes: &[u8]) -> bool {
    if bytes.len() < PLAYER_PROBE_LEN {
        return false;
    }
    let physics = u32_at(bytes, PLAYER_PHYSICS_PTR);
    plausible_position(player_position(bytes))
        && (0x1_0000..0x8000_0000).contains(&physics)
        && physics.is_multiple_of(4)
}

/// Candidates found at a constant spacing: likely an array of structs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    pub start: usize,
    pub stride: usize,
    pub count: usize,
}

/// Finds arithmetic runs among sorted candidate addresses: for each
/// candidate and each stride up to `max_stride`, how many candidates follow
/// at that spacing. Returns the runs of at least `min_count`, longest first,
/// without runs contained in a longer one.
pub fn find_runs(candidates: &[usize], max_stride: usize, min_count: usize) -> Vec<Run> {
    use std::collections::BTreeSet;
    let set: BTreeSet<usize> = candidates.iter().copied().collect();
    let mut runs = Vec::new();
    for (i, &start) in candidates.iter().enumerate() {
        for &next in &candidates[i + 1..] {
            let stride = next - start;
            if stride > max_stride {
                break;
            }
            // Only start a run at its first element.
            if start
                .checked_sub(stride)
                .is_some_and(|prev| set.contains(&prev))
            {
                continue;
            }
            let count = (0..)
                .take_while(|k| set.contains(&(start + k * stride)))
                .count();
            if count >= min_count {
                runs.push(Run {
                    start,
                    stride,
                    count,
                });
            }
        }
    }
    runs.sort_by_key(|r| (std::cmp::Reverse(r.count), r.stride));
    // Drop runs whose elements are all part of a longer, already kept run.
    let mut kept: Vec<Run> = Vec::new();
    for run in runs {
        let covered = kept.iter().any(|k| {
            run.stride.is_multiple_of(k.stride)
                && run.start >= k.start
                && (run.start - k.start).is_multiple_of(k.stride)
                && run.start + (run.count - 1) * run.stride <= k.start + (k.count - 1) * k.stride
        });
        if !covered {
            kept.push(run);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(x: f32, y: f32, z: f32, physics: u32) -> Vec<u8> {
        let mut bytes = vec![0u8; PLAYER_PROBE_LEN];
        for (i, v) in [x, y, z].iter().enumerate() {
            bytes[PLAYER_POSITION + 4 * i..PLAYER_POSITION + 4 * i + 4]
                .copy_from_slice(&v.to_le_bytes());
        }
        bytes[PLAYER_PHYSICS_PTR..PLAYER_PHYSICS_PTR + 4].copy_from_slice(&physics.to_le_bytes());
        bytes
    }

    #[test]
    fn accepts_a_player_on_the_pitch() {
        assert!(looks_like_player(&player(1200.0, 0.0, -800.0, 0x0a00_1000)));
    }

    #[test]
    fn rejects_implausible_structs() {
        assert!(!looks_like_player(&player(0.0, 0.0, 0.0, 0x0a00_1000)));
        assert!(!looks_like_player(&player(1e9, 0.0, 0.0, 0x0a00_1000)));
        assert!(!looks_like_player(&player(f32::NAN, 0.0, 0.0, 0x0a00_1000)));
        assert!(!looks_like_player(&player(100.0, 0.0, 0.0, 0)));
        assert!(!looks_like_player(&player(100.0, 0.0, 0.0, 0x0a00_1002)));
        assert!(!looks_like_player(&[0u8; 16]));
    }

    #[test]
    fn finds_the_array_of_players() {
        // 22 players every 0x6a0 bytes, plus noise.
        let mut candidates: Vec<usize> = (0..22).map(|i| 0x0100_0000 + i * 0x6a0).collect();
        candidates.extend([0x0090_0000, 0x0100_0010, 0x0200_0000]);
        candidates.sort_unstable();
        let runs = find_runs(&candidates, 0x2000, 11);
        assert_eq!(
            runs,
            vec![Run {
                start: 0x0100_0000,
                stride: 0x6a0,
                count: 22
            }]
        );
    }

    #[test]
    fn short_runs_are_ignored() {
        let candidates: Vec<usize> = (0..5).map(|i| 0x1000 + i * 0x100).collect();
        assert!(find_runs(&candidates, 0x1000, 11).is_empty());
    }
}
