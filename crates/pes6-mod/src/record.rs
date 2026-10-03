//! Recordings of real PES matches, frame by frame: the reference the Rust
//! ports are checked against (docs/BRIEF.md, R2).
//!
//! File: `CFRC`, version (u32), number of player slots (u32), then one
//! record per frame, all little-endian:
//! frame (u64), seconds since the recording started (f64), ball position
//! (3 × f32, NaN if none), then for each slot its position (3 × f32).
//! Positions are PES logic coordinates (256.5 units per metre, Y down).

use std::io::{self, Write};

pub const MAGIC: &[u8; 4] = b"CFRC";
pub const VERSION: u32 = 1;

/// One frame of a match.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub frame: u64,
    pub seconds: f64,
    pub ball: Option<[f32; 3]>,
    pub players: Vec<[f32; 3]>,
}

/// Bytes of one record for `slots` player slots.
pub fn record_len(slots: usize) -> usize {
    8 + 8 + 12 + slots * 12
}

/// Writes the file header.
pub fn write_header(out: &mut impl Write, slots: u32) -> io::Result<()> {
    out.write_all(MAGIC)?;
    out.write_all(&VERSION.to_le_bytes())?;
    out.write_all(&slots.to_le_bytes())
}

fn write_vec3(out: &mut impl Write, v: [f32; 3]) -> io::Result<()> {
    v.iter().try_for_each(|c| out.write_all(&c.to_le_bytes()))
}

/// Writes one record; `record.players` must have the header's slot count.
pub fn write_record(out: &mut impl Write, record: &Record) -> io::Result<()> {
    out.write_all(&record.frame.to_le_bytes())?;
    out.write_all(&record.seconds.to_le_bytes())?;
    write_vec3(out, record.ball.unwrap_or([f32::NAN; 3]))?;
    record.players.iter().try_for_each(|&p| write_vec3(out, p))
}

/// Reads a whole recording. A truncated last record (game closed while
/// writing) is dropped.
pub fn read(bytes: &[u8]) -> Result<Vec<Record>, String> {
    if bytes.len() < 12 || &bytes[..4] != MAGIC {
        return Err("pas un enregistrement Chaos FC".into());
    }
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let version = u32_at(4);
    if version != VERSION {
        return Err(format!("version {version} inconnue"));
    }
    let slots = u32_at(8) as usize;
    let len = record_len(slots);
    let f32_at = |b: &[u8], at: usize| f32::from_le_bytes(b[at..at + 4].try_into().unwrap());
    let vec3 = |b: &[u8], at: usize| [f32_at(b, at), f32_at(b, at + 4), f32_at(b, at + 8)];
    Ok(bytes[12..]
        .chunks_exact(len)
        .map(|r| {
            let ball = vec3(r, 16);
            Record {
                frame: u64::from_le_bytes(r[0..8].try_into().unwrap()),
                seconds: f64::from_le_bytes(r[8..16].try_into().unwrap()),
                ball: (!ball[0].is_nan()).then_some(ball),
                players: (0..slots).map(|i| vec3(r, 28 + 12 * i)).collect(),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let records = vec![
            Record {
                frame: 600,
                seconds: 0.0,
                ball: Some([1.0, -28.0, 3.0]),
                players: vec![[10.0, 0.0, 20.0], [-5.0, 0.0, 7.5]],
            },
            Record {
                frame: 601,
                seconds: 1.0 / 60.0,
                ball: None,
                players: vec![[11.0, 0.0, 20.0], [-5.0, 0.0, 7.0]],
            },
        ];
        let mut bytes = Vec::new();
        write_header(&mut bytes, 2).unwrap();
        for r in &records {
            write_record(&mut bytes, r).unwrap();
        }
        assert_eq!(bytes.len(), 12 + 2 * record_len(2));
        assert_eq!(read(&bytes).unwrap(), records);
        // A record cut in half is dropped.
        assert_eq!(read(&bytes[..bytes.len() - 5]).unwrap().len(), 1);
    }

    #[test]
    fn rejects_other_files() {
        assert!(read(b"MZ\0\0\0\0\0\0\0\0\0\0").is_err());
    }
}
