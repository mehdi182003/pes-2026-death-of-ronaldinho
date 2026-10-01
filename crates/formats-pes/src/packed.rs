//! The 32-byte header that PES 6 puts in front of most files of
//! `0_text.afs` and `e_text.afs`, followed by the file's data, compressed
//! with zlib or stored as is.
//!
//! Layout checked on the PES 6 PC demo, see `docs/formats/afs.md`.

use std::io::Read;

/// Size of the header; the data starts right after it.
pub const HEADER_SIZE: usize = 32;

/// The header of a packed file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackedHeader {
    /// Second byte. 0, 1, 2, 4, 5, 6 or 14 in the demo.
    // HYPOTHÈSE: a kind of content; files sharing a value look alike once
    // unpacked (6: a table of sub-files, 1: an image header), but its exact
    // meaning is not documented.
    pub kind: u8,
    /// Third byte: 1 for zlib data, 0 for data stored as is.
    pub compressed: bool,
    /// Bytes of data after the header.
    pub stored_size: u32,
    /// Size of the data once inflated. Only meaningful when compressed: for
    /// stored files it is often 0, but not always (its meaning is unknown).
    pub unpacked_size: u32,
}

impl PackedHeader {
    /// Reads the header at the start of `bytes`, or `None` when `bytes` does
    /// not start with one (ADX sounds, WAV files...).
    ///
    /// Recognised by its shape: bytes 0 and 3 are zero, byte 2 is 0 or 1,
    /// and the data fills the rest of the file, give or take three bytes
    /// of padding (one file of the demo has one).
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let header = bytes.get(..HEADER_SIZE)?;
        let u32_at = |at: usize| u32::from_le_bytes(header[at..at + 4].try_into().unwrap());
        if header[0] != 0 || header[3] != 0 || header[2] > 1 {
            return None;
        }
        let stored_size = u32_at(4);
        let available = bytes.len() - HEADER_SIZE;
        let padding = available.checked_sub(stored_size as usize)?;
        if padding > 3 {
            return None;
        }
        Some(Self {
            kind: header[1],
            compressed: header[2] == 1,
            stored_size,
            unpacked_size: u32_at(8),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PackedError {
    #[error("données zlib illisibles : {0}")]
    Inflate(#[source] std::io::Error),

    #[error("{found} octets décompressés au lieu des {expected} annoncés")]
    SizeMismatch { expected: u32, found: usize },
}

/// Returns the data of a packed file: inflated if compressed, as stored
/// otherwise.
pub fn unpack(header: &PackedHeader, bytes: &[u8]) -> Result<Vec<u8>, PackedError> {
    let stored = &bytes[HEADER_SIZE..HEADER_SIZE + header.stored_size as usize];
    if !header.compressed {
        return Ok(stored.to_vec());
    }
    let mut data = Vec::with_capacity(header.unpacked_size as usize);
    flate2::read::ZlibDecoder::new(stored)
        .read_to_end(&mut data)
        .map_err(PackedError::Inflate)?;
    if data.len() != header.unpacked_size as usize {
        return Err(PackedError::SizeMismatch {
            expected: header.unpacked_size,
            found: data.len(),
        });
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn header(kind: u8, compressed: bool, stored: usize, unpacked: usize) -> Vec<u8> {
        let mut bytes = vec![0, kind, u8::from(compressed), 0];
        bytes.extend((stored as u32).to_le_bytes());
        bytes.extend((unpacked as u32).to_le_bytes());
        bytes.resize(HEADER_SIZE, 0);
        bytes
    }

    fn deflate(data: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn inflates_compressed_files() {
        let data = b"PES 6 ".repeat(100);
        let zlib = deflate(&data);
        let mut file = header(6, true, zlib.len(), data.len());
        file.extend(&zlib);

        let parsed = PackedHeader::parse(&file).unwrap();
        assert_eq!(parsed.kind, 6);
        assert!(parsed.compressed);
        assert_eq!(parsed.stored_size as usize, zlib.len());
        assert_eq!(unpack(&parsed, &file).unwrap(), data);
    }

    #[test]
    fn returns_stored_files_as_is_padding_excluded() {
        let mut file = header(2, false, 5, 0);
        file.extend(b"hello\0");
        let parsed = PackedHeader::parse(&file).unwrap();
        assert!(!parsed.compressed);
        assert_eq!(unpack(&parsed, &file).unwrap(), b"hello");
    }

    #[test]
    fn other_files_have_no_header() {
        // An ADX sound starts with 0x80.
        let mut adx = vec![0x80, 0, 0, 0x24];
        adx.resize(64, 0);
        assert_eq!(PackedHeader::parse(&adx), None);
        // Too short.
        assert_eq!(PackedHeader::parse(&[0; 16]), None);
        // Stored size larger than the file.
        let mut file = header(6, true, 100, 200);
        file.extend([0; 10]);
        assert_eq!(PackedHeader::parse(&file), None);
    }

    #[test]
    fn reports_unreadable_or_short_data() {
        let mut garbage = header(6, true, 8, 100);
        garbage.extend([0xc9, 0xe2, 0xb7, 0x2c, 0xb7, 0x5d, 0xe7, 0x49]);
        let parsed = PackedHeader::parse(&garbage).unwrap();
        assert!(matches!(
            unpack(&parsed, &garbage),
            Err(PackedError::Inflate(_))
        ));

        let zlib = deflate(b"short");
        let mut file = header(6, true, zlib.len(), 999);
        file.extend(&zlib);
        let parsed = PackedHeader::parse(&file).unwrap();
        assert!(matches!(
            unpack(&parsed, &file),
            Err(PackedError::SizeMismatch {
                expected: 999,
                found: 5
            })
        ));
    }
}
