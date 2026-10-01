//! RenderWare binary streams: chunk headers, library versions and chunk
//! traversal. Shared by DFF models and, later, TXD textures.
//!
//! What was checked on real files is listed in `docs/formats/renderware.md`.

use std::fmt;
use std::io::Cursor;

use binrw::{BinRead, Endian};

/// Chunk type identifiers (GTAMods wiki, "List of RW section IDs"). Only the
/// ones met in Vice City's files are listed.
pub mod id {
    pub const STRUCT: u32 = 0x0001;
    pub const STRING: u32 = 0x0002;
    pub const EXTENSION: u32 = 0x0003;
    pub const TEXTURE: u32 = 0x0006;
    pub const MATERIAL: u32 = 0x0007;
    pub const MATERIAL_LIST: u32 = 0x0008;
    pub const FRAME_LIST: u32 = 0x000E;
    pub const GEOMETRY: u32 = 0x000F;
    pub const CLUMP: u32 = 0x0010;
    pub const LIGHT: u32 = 0x0012;
    pub const ATOMIC: u32 = 0x0014;
    pub const RASTER: u32 = 0x0015;
    pub const TEXTURE_DICTIONARY: u32 = 0x0016;
    pub const GEOMETRY_LIST: u32 = 0x001A;
    pub const RIGHT_TO_RENDER: u32 = 0x001F;
    pub const MORPH_PLG: u32 = 0x0105;
    pub const SKY_MIPMAP_VAL: u32 = 0x0110;
    pub const SKIN_PLG: u32 = 0x0116;
    pub const HANIM_PLG: u32 = 0x011E;
    pub const MATERIAL_EFFECTS_PLG: u32 = 0x0120;
    pub const BIN_MESH_PLG: u32 = 0x050E;
    /// Rockstar's frame name extension ("Frame" or "Node Name").
    pub const NODE_NAME: u32 = 0x0253_F2FE;
}

/// Human-readable name of a chunk type.
pub fn chunk_name(kind: u32) -> Option<&'static str> {
    Some(match kind {
        id::STRUCT => "Struct",
        id::STRING => "String",
        id::EXTENSION => "Extension",
        id::TEXTURE => "Texture",
        id::MATERIAL => "Material",
        id::MATERIAL_LIST => "Material List",
        id::FRAME_LIST => "Frame List",
        id::GEOMETRY => "Geometry",
        id::CLUMP => "Clump",
        id::LIGHT => "Light",
        id::ATOMIC => "Atomic",
        id::RASTER => "Raster",
        id::TEXTURE_DICTIONARY => "Texture Dictionary",
        id::GEOMETRY_LIST => "Geometry List",
        id::RIGHT_TO_RENDER => "Right To Render",
        id::MORPH_PLG => "Morph PLG",
        id::SKY_MIPMAP_VAL => "Sky Mipmap Val",
        id::SKIN_PLG => "Skin PLG",
        id::HANIM_PLG => "HAnim PLG",
        id::MATERIAL_EFFECTS_PLG => "Material Effects PLG",
        id::BIN_MESH_PLG => "Bin Mesh PLG",
        id::NODE_NAME => "Node Name",
        _ => return None,
    })
}

/// Whether the payload of this chunk type is a sequence of child chunks.
/// Checked on every DFF and TXD of Vice City.
pub fn is_container(kind: u32) -> bool {
    matches!(
        kind,
        id::EXTENSION
            | id::TEXTURE
            | id::MATERIAL
            | id::MATERIAL_LIST
            | id::FRAME_LIST
            | id::GEOMETRY
            | id::CLUMP
            | id::LIGHT
            | id::ATOMIC
            | id::RASTER
            | id::TEXTURE_DICTIONARY
            | id::GEOMETRY_LIST
    )
}

/// Header in front of every chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, BinRead)]
#[br(little)]
pub struct ChunkHeader {
    pub kind: u32,
    /// Size of the payload, header excluded.
    pub size: u32,
    /// Library ID stamp: packed RenderWare version and build number.
    pub library_id: u32,
}

impl ChunkHeader {
    pub const SIZE: usize = 12;

    pub fn version(&self) -> Version {
        Version::from_library_id(self.library_id)
    }
}

/// A RenderWare library version, e.g. 3.4.0.3 stored as `0x34003`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(pub u32);

impl Version {
    /// Unpacks a library ID stamp (GTAMods wiki, "RenderWare" page).
    pub fn from_library_id(library_id: u32) -> Self {
        if library_id & 0xFFFF_0000 != 0 {
            Version((((library_id >> 14) & 0x3_FF00) + 0x3_0000) | ((library_id >> 16) & 0x3F))
        } else {
            // Before 3.1.0.1 the stamp was the bare version, without build.
            Version(library_id << 8)
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let v = self.0;
        write!(
            f,
            "{}.{}.{}.{}",
            v >> 16,
            (v >> 12) & 0xF,
            (v >> 8) & 0xF,
            v & 0xFF
        )
    }
}

/// A chunk inside a stream.
#[derive(Debug, Clone, Copy)]
pub struct Chunk<'a> {
    pub header: ChunkHeader,
    /// Offset of the header from the start of the stream.
    pub offset: usize,
    /// The payload: the `header.size` bytes after the header.
    pub data: &'a [u8],
}

impl<'a> Chunk<'a> {
    pub fn kind(&self) -> u32 {
        self.header.kind
    }

    pub fn version(&self) -> Version {
        self.header.version()
    }

    /// Offset of the payload from the start of the stream.
    pub fn data_offset(&self) -> usize {
        self.offset + ChunkHeader::SIZE
    }

    /// Child chunks of a container chunk.
    pub fn children(&self) -> Chunks<'a> {
        Chunks::new(self.data, self.data_offset())
    }
}

/// Iterator over consecutive chunks. Stops after the first error.
pub struct Chunks<'a> {
    bytes: &'a [u8],
    base: usize,
    pos: usize,
    failed: bool,
}

impl<'a> Chunks<'a> {
    /// Chunks packed in `bytes`, which starts at offset `base` of the stream.
    pub fn new(bytes: &'a [u8], base: usize) -> Self {
        Self {
            bytes,
            base,
            pos: 0,
            failed: false,
        }
    }

    /// Offset, in the stream, of the first byte not consumed yet.
    pub fn position(&self) -> usize {
        self.base + self.pos
    }
}

impl<'a> Iterator for Chunks<'a> {
    type Item = Result<Chunk<'a>, RwError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed || self.pos >= self.bytes.len() {
            return None;
        }
        let offset = self.base + self.pos;
        let rest = &self.bytes[self.pos..];
        if rest.len() < ChunkHeader::SIZE {
            self.failed = true;
            return Some(Err(RwError::TruncatedHeader { offset }));
        }
        let header = ChunkHeader::read_le(&mut Cursor::new(rest))
            .expect("a chunk header is 12 bytes and 12 bytes are available");
        let size = header.size as usize;
        let available = rest.len() - ChunkHeader::SIZE;
        if size > available {
            self.failed = true;
            return Some(Err(RwError::ChunkOverrun {
                offset,
                kind: header.kind,
                size,
                available,
            }));
        }
        self.pos += ChunkHeader::SIZE + size;
        Some(Ok(Chunk {
            header,
            offset,
            data: &rest[ChunkHeader::SIZE..ChunkHeader::SIZE + size],
        }))
    }
}

/// The chunk at the start of `bytes`, e.g. the Clump of a DFF. What follows
/// it (such as the sector padding of an IMG entry) is ignored.
pub fn root_chunk(bytes: &[u8]) -> Result<Chunk<'_>, RwError> {
    Chunks::new(bytes, 0)
        .next()
        .unwrap_or(Err(RwError::TruncatedHeader { offset: 0 }))
}

/// A chunk and its depth in the tree, as listed by [`walk`].
#[derive(Debug, Clone, Copy)]
pub struct TreeEntry<'a> {
    pub depth: usize,
    pub chunk: Chunk<'a>,
}

/// Lists the root chunk and all its descendants, depth first.
pub fn walk(bytes: &[u8]) -> Result<Vec<TreeEntry<'_>>, RwError> {
    fn visit<'a>(
        chunk: Chunk<'a>,
        depth: usize,
        out: &mut Vec<TreeEntry<'a>>,
    ) -> Result<(), RwError> {
        out.push(TreeEntry { depth, chunk });
        if is_container(chunk.kind()) {
            for child in chunk.children() {
                visit(child?, depth + 1, out)?;
            }
        }
        Ok(())
    }

    let mut out = Vec::new();
    visit(root_chunk(bytes)?, 0, &mut out)?;
    Ok(out)
}

/// Error while reading a RenderWare stream. Offsets are counted from the
/// start of the stream.
#[derive(Debug, thiserror::Error)]
pub enum RwError {
    #[error("en-tête de chunk tronqué à l'offset {offset:#x}")]
    TruncatedHeader { offset: usize },

    #[error(
        "le chunk {} à l'offset {offset:#x} annonce {size} octets, il n'en reste que {available}",
        kind_label(.kind)
    )]
    ChunkOverrun {
        offset: usize,
        kind: u32,
        size: usize,
        available: usize,
    },

    #[error(
        "{context} : chunk {} attendu à l'offset {offset:#x}, trouvé {}",
        kind_label(.expected),
        kind_label(.found)
    )]
    UnexpectedChunk {
        context: &'static str,
        offset: usize,
        expected: u32,
        found: u32,
    },

    #[error(
        "{context} : chunk {} manquant dans le chunk à l'offset {offset:#x}",
        kind_label(.expected)
    )]
    MissingChunk {
        context: &'static str,
        offset: usize,
        expected: u32,
    },

    #[error("{context} : lecture impossible du chunk à l'offset {offset:#x} : {source}")]
    Parse {
        context: &'static str,
        offset: usize,
        #[source]
        source: binrw::Error,
    },

    #[error("{context} : {message} (chunk à l'offset {offset:#x})")]
    Invalid {
        context: &'static str,
        offset: usize,
        message: String,
    },
}

fn kind_label(kind: &u32) -> String {
    match chunk_name(*kind) {
        Some(name) => format!("{name} ({kind:#x})"),
        None => format!("{kind:#x}"),
    }
}

/// Reads `T` at the start of the payload of `chunk`. Returns the value and
/// the number of bytes consumed.
pub(crate) fn parse_prefix<T: BinRead>(
    chunk: &Chunk<'_>,
    context: &'static str,
    args: T::Args<'_>,
) -> Result<(T, usize), RwError> {
    let mut cursor = Cursor::new(chunk.data);
    let value =
        T::read_options(&mut cursor, Endian::Little, args).map_err(|source| RwError::Parse {
            context,
            offset: chunk.offset,
            source,
        })?;
    Ok((value, cursor.position() as usize))
}

/// Reads `T` from the payload of `chunk` and checks that the whole payload
/// was consumed: a leftover means the layout is not understood.
pub(crate) fn parse_exact<T: BinRead>(
    chunk: &Chunk<'_>,
    context: &'static str,
    args: T::Args<'_>,
) -> Result<T, RwError> {
    let (value, consumed) = parse_prefix(chunk, context, args)?;
    if consumed != chunk.data.len() {
        return Err(RwError::Invalid {
            context,
            offset: chunk.offset,
            message: format!("{consumed} octets lus sur {}", chunk.data.len()),
        });
    }
    Ok(value)
}

/// Rejects a count read from the file that cannot fit in the chunk (every
/// element takes at least one byte). Protects against huge allocations when
/// a file is corrupted or a field is misread.
pub(crate) fn check_count(
    chunk: &Chunk<'_>,
    context: &'static str,
    count: u32,
) -> Result<(), RwError> {
    if count as usize > chunk.data.len() {
        return Err(RwError::Invalid {
            context,
            offset: chunk.offset,
            message: format!(
                "compteur {count} incompatible avec la taille du chunk ({} octets)",
                chunk.data.len()
            ),
        });
    }
    Ok(())
}

/// Takes the next child of `parent` and checks its type.
pub(crate) fn expect_child<'a>(
    children: &mut Chunks<'a>,
    parent: &Chunk<'_>,
    kind: u32,
    context: &'static str,
) -> Result<Chunk<'a>, RwError> {
    match children.next() {
        None => Err(RwError::MissingChunk {
            context,
            offset: parent.offset,
            expected: kind,
        }),
        Some(Err(err)) => Err(err),
        Some(Ok(chunk)) if chunk.kind() == kind => Ok(chunk),
        Some(Ok(chunk)) => Err(RwError::UnexpectedChunk {
            context,
            offset: chunk.offset,
            expected: kind,
            found: chunk.kind(),
        }),
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    //! Builds synthetic RenderWare streams for tests. No game data.

    /// Library ID stamps of the versions met in Vice City.
    pub const LIB_3_2_0_0: u32 = 0x0800_FFFF;
    pub const LIB_3_3_0_2: u32 = 0x0C02_FFFF;
    pub const LIB_3_4_0_3: u32 = 0x1003_FFFF;

    pub fn chunk(kind: u32, library_id: u32, payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(12 + payload.len());
        bytes.extend_from_slice(&kind.to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&library_id.to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    pub fn container(kind: u32, library_id: u32, children: &[Vec<u8>]) -> Vec<u8> {
        chunk(kind, library_id, &children.concat())
    }

    /// Little-endian payload builder.
    #[derive(Default)]
    pub struct Payload(pub Vec<u8>);

    impl Payload {
        pub fn u16(mut self, value: u16) -> Self {
            self.0.extend_from_slice(&value.to_le_bytes());
            self
        }
        pub fn u32(mut self, value: u32) -> Self {
            self.0.extend_from_slice(&value.to_le_bytes());
            self
        }
        pub fn i32(mut self, value: i32) -> Self {
            self.0.extend_from_slice(&value.to_le_bytes());
            self
        }
        pub fn f32s(mut self, values: &[f32]) -> Self {
            for value in values {
                self.0.extend_from_slice(&value.to_le_bytes());
            }
            self
        }
        pub fn bytes(mut self, values: &[u8]) -> Self {
            self.0.extend_from_slice(values);
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_util::*;
    use super::*;

    #[test]
    fn unpacks_library_versions() {
        let version = |stamp| Version::from_library_id(stamp).to_string();
        assert_eq!(version(LIB_3_4_0_3), "3.4.0.3");
        assert_eq!(version(LIB_3_3_0_2), "3.3.0.2");
        assert_eq!(version(LIB_3_2_0_0), "3.2.0.0");
        // Example of the wiki: 3.6.0.3 is stamped 0x1803FFFF.
        assert_eq!(version(0x1803_FFFF), "3.6.0.3");
        // Old stamps hold the bare version.
        assert_eq!(Version::from_library_id(0x310), Version(0x3_1000));
        assert_eq!(Version::from_library_id(LIB_3_4_0_3), Version(0x3_4003));
    }

    #[test]
    fn walks_nested_chunks_and_ignores_trailing_padding() {
        let leaf = chunk(id::STRUCT, LIB_3_4_0_3, &[1, 2, 3, 4]);
        let name = chunk(id::NODE_NAME, LIB_3_4_0_3, b"Root");
        let ext = container(id::EXTENSION, LIB_3_4_0_3, &[name]);
        let mut stream = container(id::CLUMP, LIB_3_4_0_3, &[leaf, ext]);
        stream.extend([0; 16]);

        let tree = walk(&stream).unwrap();
        let summary: Vec<_> = tree
            .iter()
            .map(|entry| (entry.depth, entry.chunk.kind(), entry.chunk.offset))
            .collect();
        assert_eq!(
            summary,
            [
                (0, id::CLUMP, 0),
                (1, id::STRUCT, 12),
                (1, id::EXTENSION, 28),
                (2, id::NODE_NAME, 40),
            ]
        );
        assert_eq!(tree[3].chunk.data, b"Root");
        assert_eq!(tree[0].chunk.version(), Version(0x3_4003));
    }

    #[test]
    fn reports_a_chunk_larger_than_its_parent() {
        let mut leaf = chunk(id::STRUCT, LIB_3_4_0_3, &[0; 8]);
        leaf[4] = 200; // declared size: 200 bytes
        let stream = container(id::CLUMP, LIB_3_4_0_3, &[leaf]);

        let err = walk(&stream).unwrap_err();
        assert!(
            matches!(
                err,
                RwError::ChunkOverrun {
                    offset: 12,
                    size: 200,
                    available: 8,
                    ..
                }
            ),
            "{err}"
        );
        assert!(err.to_string().contains("Struct (0x1)"), "{err}");
    }

    #[test]
    fn reports_a_truncated_header() {
        assert!(matches!(
            root_chunk(&[0x10, 0, 0]),
            Err(RwError::TruncatedHeader { offset: 0 })
        ));
    }
}
