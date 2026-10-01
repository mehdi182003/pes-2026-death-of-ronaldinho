//! SFX sound banks of GTA III and Vice City (PC): a table, `audio/sfx.SDT`,
//! and the sounds themselves, `audio/sfx.RAW` (16-bit mono PCM).
//!
//! Layout checked on Vice City's bank, see `docs/formats/sfx.md`.

use std::fs::File;
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use binrw::BinRead;

/// Size of one entry of the SDT table (PC version).
pub const SDT_ENTRY_SIZE: usize = 20;

/// Where a sound lies in the RAW file, and how to play it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, BinRead)]
#[br(little)]
pub struct SoundEntry {
    /// Byte offset in the RAW file.
    pub offset: u32,
    /// Size in bytes (two bytes per sample).
    pub size: u32,
    /// Samples per second.
    pub sample_rate: u32,
    /// Where looping starts, relative to the sound (0 for most sounds).
    pub loop_start: i32,
    /// Where looping ends, -1 for the end of the sound.
    pub loop_end: i32,
}

impl SoundEntry {
    /// Length in seconds.
    pub fn duration(&self) -> f32 {
        self.size as f32 / 2.0 / self.sample_rate.max(1) as f32
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SfxError {
    #[error("impossible de lire {} : {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("table SDT de {len} octets : ce n'est pas un multiple de {SDT_ENTRY_SIZE}")]
    InvalidSdtSize { len: usize },

    #[error("le son {index} ({offset}..+{size}) dépasse la fin du fichier RAW ({raw_len} octets)")]
    OutOfBounds {
        index: usize,
        offset: u32,
        size: u32,
        raw_len: u64,
    },

    #[error("le son {index} a une taille impaire ({size} octets) : ce n'est pas du PCM 16 bits")]
    OddSize { index: usize, size: u32 },

    #[error("son {index} inexistant : la banque en contient {count}")]
    NoSuchSound { index: usize, count: usize },
}

/// Parses the content of an SDT file: 20-byte entries, without header.
pub fn parse_sdt(bytes: &[u8]) -> Result<Vec<SoundEntry>, SfxError> {
    if !bytes.len().is_multiple_of(SDT_ENTRY_SIZE) {
        return Err(SfxError::InvalidSdtSize { len: bytes.len() });
    }
    let mut cursor = Cursor::new(bytes);
    Ok((0..bytes.len() / SDT_ENTRY_SIZE)
        .map(|_| {
            SoundEntry::read_le(&mut cursor)
                .expect("the length was checked to be a multiple of the entry size")
        })
        .collect())
}

/// An open sound bank: its table, and the RAW file read on demand.
pub struct SoundBank {
    entries: Vec<SoundEntry>,
    raw_path: PathBuf,
    raw: File,
}

impl SoundBank {
    /// Opens a bank and checks that every sound lies inside the RAW file
    /// and holds whole 16-bit samples.
    pub fn open(sdt_path: &Path, raw_path: &Path) -> Result<Self, SfxError> {
        let sdt = std::fs::read(sdt_path).map_err(|source| SfxError::Io {
            path: sdt_path.to_path_buf(),
            source,
        })?;
        let entries = parse_sdt(&sdt)?;
        let raw_err = |source| SfxError::Io {
            path: raw_path.to_path_buf(),
            source,
        };
        let raw = File::open(raw_path).map_err(raw_err)?;
        let raw_len = raw.metadata().map_err(raw_err)?.len();
        for (index, entry) in entries.iter().enumerate() {
            if u64::from(entry.offset) + u64::from(entry.size) > raw_len {
                return Err(SfxError::OutOfBounds {
                    index,
                    offset: entry.offset,
                    size: entry.size,
                    raw_len,
                });
            }
            if !entry.size.is_multiple_of(2) {
                return Err(SfxError::OddSize {
                    index,
                    size: entry.size,
                });
            }
        }
        Ok(Self {
            entries,
            raw_path: raw_path.to_path_buf(),
            raw,
        })
    }

    pub fn entries(&self) -> &[SoundEntry] {
        &self.entries
    }

    /// Reads the samples of a sound: signed 16-bit, mono.
    pub fn read_samples(&mut self, index: usize) -> Result<Vec<i16>, SfxError> {
        let entry = *self.entries.get(index).ok_or(SfxError::NoSuchSound {
            index,
            count: self.entries.len(),
        })?;
        let io_err = |source| SfxError::Io {
            path: self.raw_path.clone(),
            source,
        };
        self.raw
            .seek(SeekFrom::Start(u64::from(entry.offset)))
            .map_err(io_err)?;
        let mut bytes = vec![0; entry.size as usize];
        self.raw.read_exact(&mut bytes).map_err(io_err)?;
        Ok(bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| i16::from_le_bytes(pair))
            .collect())
    }
}

/// WAV file (RIFF, PCM 16-bit mono) holding `samples`, so that a sound can
/// be played by any player. Header as given by the GTAMods wiki for
/// extracted SFX sounds.
pub fn wav_bytes(sample_rate: u32, samples: &[i16]) -> Vec<u8> {
    let data_size = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(data_size + 36).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // bytes per second
    bytes.extend_from_slice(&2u16.to_le_bytes()); // bytes per frame
    bytes.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_size.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(offset: u32, size: u32, sample_rate: u32) -> Vec<u8> {
        [offset, size, sample_rate, 0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .chain((-1i32).to_le_bytes())
            .collect()
    }

    fn write_bank(sdt: &[u8], raw: &[u8]) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let (sdt_path, raw_path) = (tmp.path().join("sfx.SDT"), tmp.path().join("sfx.RAW"));
        std::fs::write(&sdt_path, sdt).unwrap();
        std::fs::write(&raw_path, raw).unwrap();
        (tmp, sdt_path, raw_path)
    }

    #[test]
    fn parses_entries() {
        let entries = parse_sdt(&[entry(0, 4, 22050), entry(4, 2, 12000)].concat()).unwrap();
        assert_eq!(
            entries[1],
            SoundEntry {
                offset: 4,
                size: 2,
                sample_rate: 12000,
                loop_start: 0,
                loop_end: -1
            }
        );
        assert!((entries[0].duration() - 2.0 / 22050.0).abs() < 1e-9);
        assert!(matches!(
            parse_sdt(&[0; 19]),
            Err(SfxError::InvalidSdtSize { len: 19 })
        ));
    }

    #[test]
    fn reads_little_endian_samples() {
        let sdt = [entry(0, 4, 22050), entry(4, 2, 12000)].concat();
        let raw = [0x01, 0x00, 0xFF, 0xFF, 0x00, 0x80];
        let (_tmp, sdt_path, raw_path) = write_bank(&sdt, &raw);
        let mut bank = SoundBank::open(&sdt_path, &raw_path).unwrap();
        assert_eq!(bank.entries().len(), 2);
        assert_eq!(bank.read_samples(0).unwrap(), [1, -1]);
        assert_eq!(bank.read_samples(1).unwrap(), [i16::MIN]);
        assert!(matches!(
            bank.read_samples(2),
            Err(SfxError::NoSuchSound { index: 2, count: 2 })
        ));
    }

    #[test]
    fn rejects_sounds_past_the_raw_file_or_with_half_samples() {
        let (_tmp, sdt_path, raw_path) = write_bank(&entry(2, 4, 22050), &[0; 4]);
        assert!(matches!(
            SoundBank::open(&sdt_path, &raw_path),
            Err(SfxError::OutOfBounds { index: 0, .. })
        ));
        let (_tmp, sdt_path, raw_path) = write_bank(&entry(0, 3, 22050), &[0; 4]);
        assert!(matches!(
            SoundBank::open(&sdt_path, &raw_path),
            Err(SfxError::OddSize { index: 0, size: 3 })
        ));
    }

    #[test]
    fn writes_a_wav_header() {
        let wav = wav_bytes(22050, &[1, -2]);
        assert_eq!(wav.len(), 48);
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()), 40);
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 22050);
        assert_eq!(u32::from_le_bytes(wav[28..32].try_into().unwrap()), 44100);
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(&wav[44..], &[1, 0, 0xFE, 0xFF]);
    }
}
