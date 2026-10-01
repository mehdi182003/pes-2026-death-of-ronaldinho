//! AFS archives (the `dat/*.afs` files of PES 6): a header, a table of
//! (offset, size) pairs, then the files, each starting on a 2048-byte
//! boundary.
//!
//! Layout checked on the three archives of the PES 6 PC demo (`0_text.afs`,
//! `e_text.afs`, `0_sound.afs`), see `docs/formats/afs.md`.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// First four bytes of an AFS archive.
pub const MAGIC: [u8; 4] = *b"AFS\0";

/// Size of the header before the table: magic and file count.
const HEADER_SIZE: u64 = 8;

/// Size of one record of the name directory.
pub const DIRECTORY_RECORD_SIZE: u64 = 48;

/// A slot of the table. Indices never change: an empty slot (size 0) keeps
/// its place, as the game addresses files by index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AfsEntry {
    pub index: usize,
    pub offset: u32,
    pub size: u32,
}

impl AfsEntry {
    /// The PES 6 demo leaves most slots of `0_text.afs` empty (6035 of
    /// 7152): files of the full game that were removed.
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    fn end(&self) -> u64 {
        u64::from(self.offset) + u64::from(self.size)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AfsError {
    #[error("impossible de lire {} : {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("{} n'est pas une archive AFS (signature {found:02x?})", .path.display())]
    BadMagic { path: PathBuf, found: [u8; 4] },

    #[error(
        "{} : la table annonce {count} fichiers, plus que l'archive ne peut en contenir ({len} octets)",
        .path.display()
    )]
    TableTooLarge { path: PathBuf, count: u32, len: u64 },

    #[error(
        "{} : le fichier n° {index} (octets {offset}..+{size}) dépasse la fin de l'archive ({len} octets)",
        .path.display()
    )]
    EntryOutOfBounds {
        path: PathBuf,
        index: usize,
        offset: u32,
        size: u32,
        len: u64,
    },
}

/// The table of an archive, read from its first bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AfsTable {
    pub entries: Vec<AfsEntry>,
    /// The (offset, size) pair that follows the table. Public descriptions
    /// of AFS call it the position of a name directory.
    pub directory: (u32, u32),
}

impl AfsTable {
    /// Whether the pair after the table describes a usable name directory:
    /// one 48-byte record per slot, outside the data of every file.
    ///
    /// It is not the case in the PES 6 demo: in its three archives, the
    /// size is right (48 bytes per slot) but the offset falls in the middle
    /// of another file, and the bytes there are not names.
    pub fn has_directory(&self) -> bool {
        let (offset, size) = self.directory;
        let start = u64::from(offset);
        let end = start + u64::from(size);
        offset != 0
            && u64::from(size) == DIRECTORY_RECORD_SIZE * self.entries.len() as u64
            && self
                .entries
                .iter()
                .filter(|entry| !entry.is_empty())
                .all(|entry| entry.end() <= start || u64::from(entry.offset) >= end)
    }
}

/// Parses the header and the table. `bytes` holds at least the first
/// `8 + 8 × count + 8` bytes of the archive; `len` is the archive's size.
fn parse_table(path: &Path, bytes: &[u8], len: u64) -> Result<AfsTable, AfsError> {
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let count = u32_at(4) as usize;
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let at = HEADER_SIZE as usize + 8 * index;
        let entry = AfsEntry {
            index,
            offset: u32_at(at),
            size: u32_at(at + 4),
        };
        if !entry.is_empty() && entry.end() > len {
            return Err(AfsError::EntryOutOfBounds {
                path: path.to_path_buf(),
                index,
                offset: entry.offset,
                size: entry.size,
                len,
            });
        }
        entries.push(entry);
    }
    let at = HEADER_SIZE as usize + 8 * count;
    Ok(AfsTable {
        entries,
        directory: (u32_at(at), u32_at(at + 4)),
    })
}

/// An open AFS archive: its table, and the file read on demand (the
/// archives weigh up to hundreds of megabytes).
pub struct AfsArchive {
    path: PathBuf,
    file: File,
    table: AfsTable,
}

impl AfsArchive {
    pub fn open(path: &Path) -> Result<Self, AfsError> {
        let io_err = |source| AfsError::Io {
            path: path.to_path_buf(),
            source,
        };
        let mut file = File::open(path).map_err(io_err)?;
        let len = file.metadata().map_err(io_err)?.len();

        let mut header = [0u8; HEADER_SIZE as usize];
        file.read_exact(&mut header).map_err(io_err)?;
        let found: [u8; 4] = header[..4].try_into().unwrap();
        if found != MAGIC {
            return Err(AfsError::BadMagic {
                path: path.to_path_buf(),
                found,
            });
        }
        let count = u32::from_le_bytes(header[4..].try_into().unwrap());
        // The table, plus the directory pair after it.
        let table_len = HEADER_SIZE + 8 * u64::from(count) + 8;
        if table_len > len {
            return Err(AfsError::TableTooLarge {
                path: path.to_path_buf(),
                count,
                len,
            });
        }
        let mut bytes = vec![0u8; table_len as usize];
        bytes[..header.len()].copy_from_slice(&header);
        file.read_exact(&mut bytes[header.len()..])
            .map_err(io_err)?;
        let table = parse_table(path, &bytes, len)?;
        Ok(Self {
            path: path.to_path_buf(),
            file,
            table,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every slot, empty ones included.
    pub fn entries(&self) -> &[AfsEntry] {
        &self.table.entries
    }

    pub fn table(&self) -> &AfsTable {
        &self.table
    }

    /// Reads the bytes of a file (empty for an empty slot).
    pub fn read(&mut self, entry: &AfsEntry) -> Result<Vec<u8>, AfsError> {
        let io_err = |source| AfsError::Io {
            path: self.path.clone(),
            source,
        };
        let mut data = vec![0; entry.size as usize];
        if entry.is_empty() {
            return Ok(data);
        }
        self.file
            .seek(SeekFrom::Start(u64::from(entry.offset)))
            .map_err(io_err)?;
        self.file.read_exact(&mut data).map_err(io_err)?;
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an archive the way PES 6 stores it: files on 2048-byte
    /// boundaries after the table.
    fn build(files: &[&[u8]], directory: (u32, u32)) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend((files.len() as u32).to_le_bytes());
        let mut offset = 2048u32;
        let mut data = Vec::new();
        for file in files {
            if file.is_empty() {
                // Empty slots of the demo still point somewhere.
                bytes.extend(offset.to_le_bytes());
                bytes.extend(0u32.to_le_bytes());
                continue;
            }
            bytes.extend(offset.to_le_bytes());
            bytes.extend((file.len() as u32).to_le_bytes());
            data.extend_from_slice(file);
            data.resize(data.len().next_multiple_of(2048), 0);
            offset = 2048 + data.len() as u32;
        }
        bytes.extend(directory.0.to_le_bytes());
        bytes.extend(directory.1.to_le_bytes());
        bytes.resize(2048, 0);
        bytes.extend(data);
        bytes
    }

    fn write(bytes: &[u8]) -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test.afs");
        std::fs::write(&path, bytes).unwrap();
        (tmp, path)
    }

    #[test]
    fn reads_files_and_keeps_empty_slots() {
        let (_tmp, path) = write(&build(&[b"first", b"", b"third file"], (0, 0)));
        let mut archive = AfsArchive::open(&path).unwrap();
        let entries = archive.entries().to_vec();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].offset, 2048);
        assert!(entries[1].is_empty());
        assert_eq!(entries[2].index, 2);
        assert_eq!(entries[2].offset, 4096);
        assert_eq!(archive.read(&entries[0]).unwrap(), b"first");
        assert_eq!(archive.read(&entries[1]).unwrap(), b"");
        assert_eq!(archive.read(&entries[2]).unwrap(), b"third file");
        assert!(!archive.table().has_directory());
    }

    #[test]
    fn directory_pair_must_lie_outside_the_files() {
        let bytes = build(&[b"a", b"b"], (0, 0));
        let table = |directory| AfsTable {
            directory,
            ..parse_table(Path::new("t.afs"), &bytes, bytes.len() as u64).unwrap()
        };
        // After the files, one record per slot: usable.
        assert!(table((6144, 96)).has_directory());
        // Inside the second file, as in the PES 6 demo.
        assert!(!table((4096, 96)).has_directory());
        // Wrong size for two slots.
        assert!(!table((6144, 48)).has_directory());
    }

    #[test]
    fn rejects_other_files() {
        let (_tmp, path) = write(b"RIFF\0\0\0\0WAVE");
        assert!(matches!(
            AfsArchive::open(&path),
            Err(AfsError::BadMagic { found, .. }) if &found == b"RIFF"
        ));
    }

    #[test]
    fn rejects_tables_and_files_past_the_end() {
        let mut bytes = build(&[b"a"], (0, 0));
        bytes[4..8].copy_from_slice(&1_000_000u32.to_le_bytes());
        let (_tmp, path) = write(&bytes);
        assert!(matches!(
            AfsArchive::open(&path),
            Err(AfsError::TableTooLarge {
                count: 1_000_000,
                ..
            })
        ));

        let mut bytes = build(&[b"a"], (0, 0));
        bytes[12..16].copy_from_slice(&5000u32.to_le_bytes());
        let (_tmp, path) = write(&bytes);
        assert!(matches!(
            AfsArchive::open(&path),
            Err(AfsError::EntryOutOfBounds { index: 0, .. })
        ));
    }
}
