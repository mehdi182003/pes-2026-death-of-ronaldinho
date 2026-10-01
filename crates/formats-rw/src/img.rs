//! IMG archives, version 1 (GTA III and Vice City): a `.dir` directory file
//! next to an `.img` data file.
//!
//! Layout checked on `models/gta3.dir` / `gta3.img` of Vice City PC, see
//! `docs/formats/img.md`.

use std::fs::File;
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use binrw::BinRead;

use crate::text::latin1_until_nul;

/// Size of a sector, the unit of offsets and sizes in the directory.
pub const SECTOR_SIZE: u64 = 2048;

/// Size of one directory entry in the `.dir` file.
pub const DIR_ENTRY_SIZE: usize = 32;

/// One entry of the `.dir` file, as stored on disk.
#[derive(BinRead)]
#[br(little)]
struct RawDirEntry {
    offset_sectors: u32,
    size_sectors: u32,
    /// NUL-terminated. Bytes after the terminator are leftovers from the
    /// tool that built the archive and must be ignored.
    name: [u8; 24],
}

/// A file stored in an IMG archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub offset_sectors: u32,
    pub size_sectors: u32,
}

impl DirEntry {
    pub fn byte_offset(&self) -> u64 {
        u64::from(self.offset_sectors) * SECTOR_SIZE
    }

    /// Size rounded up to whole sectors: the real file ends with padding.
    pub fn byte_size(&self) -> u64 {
        u64::from(self.size_sectors) * SECTOR_SIZE
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ImgError {
    #[error("impossible de lire {} : {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error(
        "taille du répertoire invalide ({len} octets) : ce n'est pas un multiple de {DIR_ENTRY_SIZE}"
    )]
    InvalidDirSize { len: usize },

    #[error(
        "l'entrée {name} (secteurs {offset_sectors}..+{size_sectors}) dépasse la fin de l'archive ({img_len} octets)"
    )]
    EntryOutOfBounds {
        name: String,
        offset_sectors: u32,
        size_sectors: u32,
        img_len: u64,
    },
}

/// Parses the content of a `.dir` file: a plain array of 32-byte entries,
/// without any header.
pub fn parse_dir(bytes: &[u8]) -> Result<Vec<DirEntry>, ImgError> {
    if !bytes.len().is_multiple_of(DIR_ENTRY_SIZE) {
        return Err(ImgError::InvalidDirSize { len: bytes.len() });
    }
    let count = bytes.len() / DIR_ENTRY_SIZE;
    let mut cursor = Cursor::new(bytes);
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let raw = RawDirEntry::read_le(&mut cursor)
            .expect("the length was checked to be a multiple of the entry size");
        entries.push(DirEntry {
            name: latin1_until_nul(&raw.name),
            offset_sectors: raw.offset_sectors,
            size_sectors: raw.size_sectors,
        });
    }
    Ok(entries)
}

/// An open IMG archive: its directory, and the data file read on demand.
pub struct ImgArchive {
    entries: Vec<DirEntry>,
    img_path: PathBuf,
    img: File,
}

impl ImgArchive {
    /// Opens `dir_path` and checks that every entry lies inside `img_path`.
    pub fn open(dir_path: &Path, img_path: &Path) -> Result<Self, ImgError> {
        let dir_bytes = std::fs::read(dir_path).map_err(|source| ImgError::Io {
            path: dir_path.to_path_buf(),
            source,
        })?;
        let entries = parse_dir(&dir_bytes)?;
        let img_err = |source| ImgError::Io {
            path: img_path.to_path_buf(),
            source,
        };
        let img = File::open(img_path).map_err(img_err)?;
        let img_len = img.metadata().map_err(img_err)?.len();
        if let Some(entry) = entries
            .iter()
            .find(|entry| entry.byte_offset() + entry.byte_size() > img_len)
        {
            return Err(ImgError::EntryOutOfBounds {
                name: entry.name.clone(),
                offset_sectors: entry.offset_sectors,
                size_sectors: entry.size_sectors,
                img_len,
            });
        }
        Ok(Self {
            entries,
            img_path: img_path.to_path_buf(),
            img,
        })
    }

    /// Opens `<path>.dir` and `<path>.img`, e.g. `models/gta3`.
    pub fn open_pair(path_without_extension: &Path) -> Result<Self, ImgError> {
        Self::open(
            &path_without_extension.with_extension("dir"),
            &path_without_extension.with_extension("img"),
        )
    }

    pub fn entries(&self) -> &[DirEntry] {
        &self.entries
    }

    /// Finds an entry by name, ignoring case (the archive mixes cases).
    ///
    /// Some names appear twice with different contents (11 in Vice City's
    /// `gta3.img`, e.g. `chef.dff`). This returns the first one.
    // HYPOTHÈSE: which duplicate the game actually loads is unknown; the
    // first entry is used until an in-game observation says otherwise.
    pub fn find(&self, name: &str) -> Option<&DirEntry> {
        self.entries
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
    }

    /// Reads the content of an entry, sector padding included.
    pub fn read(&mut self, entry: &DirEntry) -> Result<Vec<u8>, ImgError> {
        let io_err = |source| ImgError::Io {
            path: self.img_path.clone(),
            source,
        };
        self.img
            .seek(SeekFrom::Start(entry.byte_offset()))
            .map_err(io_err)?;
        let len = usize::try_from(entry.byte_size()).expect("entry sizes fit in memory");
        let mut data = vec![0; len];
        self.img.read_exact(&mut data).map_err(io_err)?;
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a directory entry the way the archive stores it.
    fn raw_entry(offset: u32, size: u32, name: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(DIR_ENTRY_SIZE);
        bytes.extend_from_slice(&offset.to_le_bytes());
        bytes.extend_from_slice(&size.to_le_bytes());
        let mut field = [0u8; 24];
        field[..name.len()].copy_from_slice(name);
        bytes.extend_from_slice(&field);
        bytes
    }

    #[test]
    fn parses_entries_and_ignores_bytes_after_the_terminator() {
        let mut dir = raw_entry(0, 5, b"radar00.txd");
        // Leftover bytes after the NUL, as seen in the real archive.
        dir.extend(raw_entry(5, 43, b"Player.dff\0junk"));

        let entries = parse_dir(&dir).unwrap();
        assert_eq!(
            entries,
            [
                DirEntry {
                    name: "radar00.txd".into(),
                    offset_sectors: 0,
                    size_sectors: 5
                },
                DirEntry {
                    name: "Player.dff".into(),
                    offset_sectors: 5,
                    size_sectors: 43
                },
            ]
        );
        assert_eq!(entries[1].byte_offset(), 5 * 2048);
        assert_eq!(entries[1].byte_size(), 43 * 2048);
    }

    #[test]
    fn name_filling_the_whole_field_is_kept() {
        let entries = parse_dir(&raw_entry(0, 1, &[b'a'; 24])).unwrap();
        assert_eq!(entries[0].name, "a".repeat(24));
    }

    #[test]
    fn rejects_truncated_directory() {
        let dir = raw_entry(0, 1, b"a.dff");
        assert!(matches!(
            parse_dir(&dir[..31]),
            Err(ImgError::InvalidDirSize { len: 31 })
        ));
    }

    fn write_archive(dir: &[u8], img: &[u8]) -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("test");
        std::fs::write(base.with_extension("dir"), dir).unwrap();
        std::fs::write(base.with_extension("img"), img).unwrap();
        (tmp, base)
    }

    #[test]
    fn finds_entries_ignoring_case_and_reads_them() {
        let mut dir = raw_entry(0, 1, b"first.dff");
        dir.extend(raw_entry(1, 1, b"Second.TXD"));
        dir.extend(raw_entry(2, 1, b"second.txd"));
        let mut img = vec![1u8; 2048];
        img.extend([2u8; 2048]);
        img.extend([3u8; 2048]);
        let (_tmp, base) = write_archive(&dir, &img);

        let mut archive = ImgArchive::open_pair(&base).unwrap();
        assert_eq!(archive.entries().len(), 3);
        let entry = archive.find("SECOND.txd").unwrap().clone();
        // The first of two entries with the same name wins.
        assert_eq!(entry.offset_sectors, 1);
        assert_eq!(archive.read(&entry).unwrap(), vec![2u8; 2048]);
        assert!(archive.find("missing.dff").is_none());
    }

    #[test]
    fn rejects_entries_past_the_end_of_the_data_file() {
        let (_tmp, base) = write_archive(&raw_entry(1, 2, b"big.dff"), &[0u8; 2048]);
        let Err(err) = ImgArchive::open_pair(&base) else {
            panic!("an out-of-bounds entry must be rejected");
        };
        assert!(
            matches!(err, ImgError::EntryOutOfBounds { ref name, .. } if name == "big.dff"),
            "{err}"
        );
    }
}
