//! What the files of the AFS archives contain: sub-file containers and the
//! signatures of the formats found inside, recognised without file names
//! (the names of the archives are not readable).
//!
//! Signatures checked on the full PES 6 PC game, see `docs/formats/afs.md`.

use std::ops::Range;

use crate::packed::{self, PackedHeader};

/// Signature of the 3D models: first four bytes `20 05 04 20`.
// HYPOTHÈSE: these are models. In the face and hairstyle slots of the
// community map of 0_text.afs, each file is a container of two of them and
// one texture; their layout is read in J5.
pub const MODEL_MAGIC: [u8; 4] = [0x20, 0x05, 0x04, 0x20];

/// Signature of the textures: first four bytes `94 72 85 29`.
// HYPOTHÈSE: these are textures. They fill the kit, number and palette
// slots of the community map, and every header of the game gives a width
// and a height (u16 at 20 and 22) with their base-2 logarithms rounded up
// (bytes 26 and 27); their pixels are read in J5.
pub const TEXTURE_MAGIC: [u8; 4] = [0x94, 0x72, 0x85, 0x29];

/// Copyright marker of CRI's ADX sounds, before the audio data.
const ADX_COPYRIGHT: &[u8] = b"(c)CRI";

/// Nesting limit when looking inside containers and packed files.
const MAX_DEPTH: usize = 4;

/// The kind of content of a file, once unpacked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A table of sub-files (see [`parse_container`]).
    Container,
    /// [`MODEL_MAGIC`].
    Model,
    /// [`TEXTURE_MAGIC`].
    Texture,
    /// CRI ADX sound: `80 00`, then the big-endian offset of the audio data,
    /// preceded by `(c)CRI`.
    Adx,
    /// RIFF WAVE sound.
    Wav,
    /// Player database, starting with `WEPLDATA`.
    PlayerData,
    /// Nothing in the file.
    Empty,
    /// No known signature: the first bytes (up to four).
    Unknown(Vec<u8>),
}

impl Kind {
    /// Short name shown to the player in the tools.
    pub fn label(&self) -> String {
        match self {
            Kind::Container => "conteneur".into(),
            Kind::Model => "modèle".into(),
            Kind::Texture => "texture".into(),
            Kind::Adx => "son ADX".into(),
            Kind::Wav => "son WAV".into(),
            Kind::PlayerData => "base des joueurs".into(),
            Kind::Empty => "vide".into(),
            Kind::Unknown(head) => {
                let hex: Vec<String> = head.iter().map(|byte| format!("{byte:02x}")).collect();
                format!("inconnu ({})", hex.join(" "))
            }
        }
    }

    /// File extension for extracted files. `mdl` and `tex` are ours: the
    /// game has no file names.
    pub fn extension(&self) -> &'static str {
        match self {
            Kind::Model => "mdl",
            Kind::Texture => "tex",
            Kind::Adx => "adx",
            Kind::Wav => "wav",
            _ => "bin",
        }
    }
}

/// How a file is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packing {
    /// No packed header: the bytes are the content (ADX, WAV...).
    Plain,
    /// Packed header, data stored as is.
    Stored,
    /// Packed header, zlib data.
    Compressed,
    /// Packed header, but the data could not be inflated.
    Unreadable(String),
}

/// What a file contains, and the sub-files of a container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub packing: Packing,
    pub kind: Kind,
    /// Size of the content once unpacked (stored size if unreadable).
    pub size: usize,
    pub children: Vec<Report>,
}

impl Report {
    /// This report and every report below it, depth first.
    pub fn walk(&self) -> Vec<&Report> {
        let mut all = vec![self];
        for child in &self.children {
            all.extend(child.walk());
        }
        all
    }
}

/// Unpacks `bytes` when needed and identifies the content, looking inside
/// containers and packed sub-files.
pub fn inspect(bytes: &[u8]) -> Report {
    inspect_at(bytes, 0)
}

fn inspect_at(bytes: &[u8], depth: usize) -> Report {
    let (packing, content) = unpack_any(bytes);
    let Some(content) = content else {
        return Report {
            packing,
            kind: Kind::Unknown(Vec::new()),
            size: bytes.len().saturating_sub(packed::HEADER_SIZE),
            children: Vec::new(),
        };
    };
    let kind = identify(&content);
    let children = sub_files(&kind, &content, depth)
        .map(|sub| inspect_at(sub, depth + 1))
        .collect();
    Report {
        packing,
        kind,
        size: content.len(),
        children,
    }
}

/// A file or sub-file, unpacked, as written by the extraction tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extracted {
    /// Position among the sub-files at each level: empty for the file
    /// itself, `[1]` for its second sub-file, `[1, 0]` for the first
    /// sub-file of that one.
    pub path: Vec<usize>,
    pub kind: Kind,
    /// Unpacked content; the stored bytes, header included, if unreadable.
    pub data: Vec<u8>,
}

/// Unpacks `bytes` and every sub-file inside, depth first: the file itself
/// comes first.
pub fn extract(bytes: &[u8]) -> Vec<Extracted> {
    let mut all = Vec::new();
    extract_at(bytes, Vec::new(), &mut all);
    all
}

fn extract_at(bytes: &[u8], path: Vec<usize>, all: &mut Vec<Extracted>) {
    let depth = path.len();
    let Some(content) = unpack_any(bytes).1 else {
        all.push(Extracted {
            path,
            kind: Kind::Unknown(Vec::new()),
            data: bytes.to_vec(),
        });
        return;
    };
    let kind = identify(&content);
    let subs: Vec<&[u8]> = sub_files(&kind, &content, depth).collect();
    all.push(Extracted {
        path: path.clone(),
        kind,
        data: content.clone(),
    });
    for (position, sub) in subs.into_iter().enumerate() {
        let mut sub_path = path.clone();
        sub_path.push(position);
        extract_at(sub, sub_path, all);
    }
}

/// How `bytes` is stored, and its content once unpacked (`None` if the data
/// cannot be inflated).
fn unpack_any(bytes: &[u8]) -> (Packing, Option<Vec<u8>>) {
    match PackedHeader::parse(bytes) {
        None => (Packing::Plain, Some(bytes.to_vec())),
        Some(header) => match packed::unpack(&header, bytes) {
            Ok(data) if header.compressed => (Packing::Compressed, Some(data)),
            Ok(data) => (Packing::Stored, Some(data)),
            Err(err) => (Packing::Unreadable(err.to_string()), None),
        },
    }
}

/// The sub-files of a container, unless the nesting limit is reached.
fn sub_files<'a>(kind: &Kind, content: &'a [u8], depth: usize) -> impl Iterator<Item = &'a [u8]> {
    let ranges = match kind {
        Kind::Container if depth < MAX_DEPTH => parse_container(content).unwrap_or_default(),
        _ => Vec::new(),
    };
    ranges.into_iter().map(move |range| &content[range])
}

/// Recognises unpacked content by its first bytes.
pub fn identify(data: &[u8]) -> Kind {
    if data.is_empty() {
        return Kind::Empty;
    }
    if data.starts_with(&MODEL_MAGIC) {
        return Kind::Model;
    }
    if data.starts_with(&TEXTURE_MAGIC) {
        return Kind::Texture;
    }
    if is_adx(data) {
        return Kind::Adx;
    }
    if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WAVE") {
        return Kind::Wav;
    }
    if data.starts_with(b"WEPLDATA") {
        return Kind::PlayerData;
    }
    if parse_container(data).is_some() {
        return Kind::Container;
    }
    Kind::Unknown(data[..data.len().min(4)].to_vec())
}

fn is_adx(data: &[u8]) -> bool {
    let Some(&[0x80, 0x00, high, low]) = data.get(..4) else {
        return false;
    };
    let audio = usize::from(u16::from_be_bytes([high, low]));
    audio >= ADX_COPYRIGHT.len() + 2 && data.get(audio - 2..audio + 4) == Some(ADX_COPYRIGHT)
}

/// Splits a container into the byte ranges of its sub-files.
///
/// Layout: number of sub-files (u32), position of the offset table (u32,
/// always 8), then one u32 offset per sub-file, in increasing order. A
/// sub-file ends where the next one starts, the last one at the end of the
/// data. 7703 of the 7713 readable files of kind 6 of `0_text.afs` match.
///
/// An offset of 0 is an empty slot: it keeps its position, and the other
/// sub-files end where the next non-empty one starts. Stadium containers
/// (0_text:6949 and the like) have many.
pub fn parse_container(data: &[u8]) -> Option<Vec<Range<usize>>> {
    let u32_at = |at: usize| {
        data.get(at..at + 4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()) as usize)
    };
    let count = u32_at(0)?;
    if count == 0 || u32_at(4)? != 8 {
        return None;
    }
    let table_end = 8usize.checked_add(count.checked_mul(4)?)?;
    let offsets = (0..count)
        .map(|index| u32_at(8 + 4 * index))
        .collect::<Option<Vec<usize>>>()?;
    let used: Vec<usize> = offsets.iter().copied().filter(|&o| o != 0).collect();
    let ordered = used.windows(2).all(|pair| pair[0] <= pair[1]);
    let (Some(&first), Some(&last)) = (used.first(), used.last()) else {
        return None;
    };
    if first < table_end || !ordered || last > data.len() {
        return None;
    }
    let mut next = used.iter().skip(1).copied().chain([data.len()]);
    Some(
        offsets
            .iter()
            .map(|&start| match start {
                0 => table_end..table_end,
                _ => start..next.next().unwrap_or(data.len()),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn container(files: &[&[u8]]) -> Vec<u8> {
        let mut bytes = (files.len() as u32).to_le_bytes().to_vec();
        bytes.extend(8u32.to_le_bytes());
        let mut offset = 8 + 4 * files.len();
        for file in files {
            bytes.extend((offset as u32).to_le_bytes());
            offset += file.len();
        }
        for file in files {
            bytes.extend_from_slice(file);
        }
        bytes
    }

    fn adx() -> Vec<u8> {
        let mut bytes = vec![0x80, 0x00, 0x00, 0x24];
        bytes.resize(0x22, 0);
        bytes.extend(b"(c)CRI");
        bytes
    }

    fn compressed(data: &[u8]) -> Vec<u8> {
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data).unwrap();
        let zlib = encoder.finish().unwrap();
        let mut bytes = vec![0, 6, 1, 0];
        bytes.extend((zlib.len() as u32).to_le_bytes());
        bytes.extend((data.len() as u32).to_le_bytes());
        bytes.resize(packed::HEADER_SIZE, 0);
        bytes.extend(zlib);
        bytes
    }

    #[test]
    fn recognises_signatures() {
        assert_eq!(identify(&[0x20, 0x05, 0x04, 0x20, 1]), Kind::Model);
        assert_eq!(identify(&[0x94, 0x72, 0x85, 0x29, 1]), Kind::Texture);
        assert_eq!(identify(&adx()), Kind::Adx);
        assert_eq!(identify(b"RIFF\x10\0\0\0WAVEfmt "), Kind::Wav);
        assert_eq!(identify(b"WEPLDATA..."), Kind::PlayerData);
        assert_eq!(identify(b""), Kind::Empty);
        assert_eq!(identify(b"aPDT\0\0"), Kind::Unknown(b"aPDT".to_vec()));
        // 80 00 without the CRI marker is not an ADX sound.
        assert_eq!(
            identify(&[0x80, 0, 0, 4, 9, 9, 9, 9]).label(),
            "inconnu (80 00 00 04)"
        );
    }

    #[test]
    fn splits_containers_and_rejects_other_tables() {
        let data = container(&[b"abc", b"", b"defg"]);
        assert_eq!(parse_container(&data).unwrap(), [20..23, 23..23, 23..27]);
        // Table position other than 8.
        let mut other = data.clone();
        other[4] = 12;
        assert_eq!(parse_container(&other), None);
        // Offsets going backwards.
        let mut backwards = data.clone();
        backwards[12..16].copy_from_slice(&2u32.to_le_bytes());
        assert_eq!(parse_container(&backwards), None);
        // Offset past the end.
        let mut past = data.clone();
        past[16..20].copy_from_slice(&99u32.to_le_bytes());
        assert_eq!(parse_container(&past), None);
        // Empty slots (offset 0) keep their position.
        let mut gaps = data;
        gaps[12..16].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(parse_container(&gaps).unwrap(), [20..23, 20..20, 23..27]);
        let mut empty = container(&[b"ab"]);
        empty[8..12].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(parse_container(&empty), None);
    }

    #[test]
    fn looks_inside_packed_containers() {
        let model = [0x20, 0x05, 0x04, 0x20, 7, 7];
        let inner = compressed(&container(&[&[0x94, 0x72, 0x85, 0x29, 0]]));
        let outer = compressed(&container(&[&model, &inner]));

        let report = inspect(&outer);
        assert_eq!(report.packing, Packing::Compressed);
        assert_eq!(report.kind, Kind::Container);
        assert_eq!(report.children.len(), 2);
        assert_eq!(report.children[0].kind, Kind::Model);
        assert_eq!(report.children[0].packing, Packing::Plain);
        let nested = &report.children[1];
        assert_eq!(nested.packing, Packing::Compressed);
        assert_eq!(nested.children[0].kind, Kind::Texture);
        assert_eq!(report.walk().len(), 4);

        let extracted = extract(&outer);
        let paths: Vec<&[usize]> = extracted.iter().map(|file| &file.path[..]).collect();
        assert_eq!(paths, [&[][..], &[0], &[1], &[1, 0]]);
        assert_eq!(extracted[1].data, model);
        assert_eq!(extracted[3].kind.extension(), "tex");
        assert_eq!(&extracted[3].data[..4], TEXTURE_MAGIC);
    }

    #[test]
    fn unreadable_data_is_reported_not_guessed() {
        let mut bytes = vec![0, 6, 1, 0];
        bytes.extend(4u32.to_le_bytes());
        bytes.extend(100u32.to_le_bytes());
        bytes.resize(packed::HEADER_SIZE, 0);
        bytes.extend([0xc9, 0xe2, 0xb7, 0x2c]);
        let report = inspect(&bytes);
        assert!(matches!(report.packing, Packing::Unreadable(_)));
        assert!(report.children.is_empty());
    }
}
