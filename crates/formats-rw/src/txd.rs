//! TXD texture dictionaries: PC (Direct3D 8) native rasters of Vice City.
//!
//! Checked on the 1399 TXD files of Vice City (`gta3.img` and loose files),
//! see `docs/formats/txd.md`.

use binrw::BinRead;

use crate::rw::{Chunk, RwError, Version, expect_child, id, parse_exact, parse_prefix, root_chunk};
use crate::text::latin1_until_nul;

/// Bits of [`NativeTexture::raster_format`].
pub mod raster_format {
    /// Mask of the pixel format part.
    pub const PIXEL_MASK: u32 = 0x0F00;
    /// 1-bit alpha, RGB 5 bits each; DXT1 with alpha when compressed.
    pub const FORMAT_1555: u32 = 0x0100;
    /// RGB 5-6-5; DXT1 without alpha when compressed.
    pub const FORMAT_565: u32 = 0x0200;
    /// RGBA 4 bits each; DXT3 when compressed.
    pub const FORMAT_4444: u32 = 0x0300;
    pub const FORMAT_8888: u32 = 0x0500;
    pub const FORMAT_888: u32 = 0x0600;
    pub const AUTO_MIPMAP: u32 = 0x1000;
    pub const PAL8: u32 = 0x2000;
    pub const PAL4: u32 = 0x4000;
    pub const MIPMAP: u32 = 0x8000;
}

/// Platform ID of PC rasters in GTA III and Vice City (Direct3D 8).
pub const PLATFORM_D3D8: u32 = 8;

/// A parsed TXD file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureDictionary {
    pub version: Version,
    pub textures: Vec<NativeTexture>,
}

impl TextureDictionary {
    /// Finds a texture by name, ignoring case like the game.
    pub fn find(&self, name: &str) -> Option<&NativeTexture> {
        self.textures
            .iter()
            .find(|texture| texture.name.eq_ignore_ascii_case(name))
    }
}

/// A texture as stored in a TXD, before decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeTexture {
    pub name: String,
    pub mask_name: String,
    /// Filtering and addressing, same layout as in DFF textures.
    pub sampler: u32,
    /// [`raster_format`] bits.
    pub raster_format: u32,
    pub has_alpha: bool,
    pub width: u16,
    pub height: u16,
    /// Bits per pixel: 8 with a palette, 16 for DXT, 32 for raw colours.
    pub depth: u8,
    /// 4 (texture) in every Vice City file.
    pub raster_type: u8,
    /// 0 none, 1 DXT1, 3 DXT3.
    pub compression: u8,
    /// 256 colours of 4 bytes, for 8-bit palettized textures.
    pub palette: Option<Vec<[u8; 4]>>,
    /// Pixel data of each mipmap level, largest first, as stored.
    pub levels: Vec<Vec<u8>>,
}

#[derive(BinRead)]
#[br(little)]
struct RasterHeader {
    platform: u32,
    sampler: u32,
    name: [u8; 32],
    mask_name: [u8; 32],
    raster_format: u32,
    has_alpha: u32,
    width: u16,
    height: u16,
    depth: u8,
    level_count: u8,
    raster_type: u8,
    compression: u8,
}

/// 88 bytes; the GTAMods wiki says 86, but its own fields add up to 88, and
/// so do the real files.
const RASTER_HEADER_SIZE: usize = 88;

/// Parses a TXD file. Bytes after the root chunk (IMG padding) are ignored.
pub fn parse_txd(bytes: &[u8]) -> Result<TextureDictionary, RwError> {
    const CONTEXT: &str = "Texture Dictionary";
    let root = root_chunk(bytes)?;
    if root.kind() != id::TEXTURE_DICTIONARY {
        return Err(RwError::UnexpectedChunk {
            context: "TXD",
            offset: root.offset,
            expected: id::TEXTURE_DICTIONARY,
            found: root.kind(),
        });
    }
    let mut children = root.children();
    // A u32 count before RenderWare 3.6 (count and device ID afterwards).
    let count = parse_exact::<u32>(
        &expect_child(&mut children, &root, id::STRUCT, CONTEXT)?,
        CONTEXT,
        (),
    )?;
    let textures = (0..count)
        .map(|_| parse_raster(&expect_child(&mut children, &root, id::RASTER, CONTEXT)?))
        .collect::<Result<_, _>>()?;
    Ok(TextureDictionary {
        version: root.version(),
        textures,
    })
}

fn parse_raster(raster: &Chunk<'_>) -> Result<NativeTexture, RwError> {
    const CONTEXT: &str = "Raster";
    let mut children = raster.children();
    let data = expect_child(&mut children, raster, id::STRUCT, CONTEXT)?;
    let invalid = |message: String| RwError::Invalid {
        context: CONTEXT,
        offset: data.offset,
        message,
    };

    let (header, _) = parse_prefix::<RasterHeader>(&data, CONTEXT, ())?;
    if header.platform != PLATFORM_D3D8 {
        return Err(invalid(format!(
            "plateforme {} non gérée (8 attendu)",
            header.platform
        )));
    }
    let mut rest = &data.data[RASTER_HEADER_SIZE..];
    let mut take = |len: usize, what: &str| -> Result<&[u8], RwError> {
        if rest.len() < len {
            return Err(invalid(format!("{what} tronqué")));
        }
        let (head, tail) = rest.split_at(len);
        rest = tail;
        Ok(head)
    };

    let palette = if header.raster_format & raster_format::PAL8 != 0 {
        let bytes = take(256 * 4, "palette")?;
        Some(bytes.as_chunks::<4>().0.to_vec())
    } else if header.raster_format & raster_format::PAL4 != 0 {
        // Never met in Vice City PC files: its palette size is not known.
        return Err(invalid("palette 4 bits non gérée".into()));
    } else {
        None
    };

    let mut levels = Vec::with_capacity(usize::from(header.level_count));
    for _ in 0..header.level_count {
        let size = u32::from_le_bytes(take(4, "taille de niveau")?.try_into().unwrap());
        levels.push(take(size as usize, "niveau de mipmap")?.to_vec());
    }
    if !rest.is_empty() {
        return Err(invalid(format!("{} octets finaux non lus", rest.len())));
    }

    Ok(NativeTexture {
        name: latin1_until_nul(&header.name),
        mask_name: latin1_until_nul(&header.mask_name),
        sampler: header.sampler,
        raster_format: header.raster_format,
        has_alpha: header.has_alpha != 0,
        width: header.width,
        height: header.height,
        depth: header.depth,
        raster_type: header.raster_type,
        compression: header.compression,
        palette,
        levels,
    })
}

/// A decoded image: 4 bytes (R, G, B, A) per pixel, rows from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba8Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Why a texture cannot be decoded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("{name} : aucun niveau d'image")]
    NoLevel { name: String },

    #[error(
        "{name} : format {raster_format:#06x} (compression {compression}, {depth} bits) non géré"
    )]
    Unsupported {
        name: String,
        raster_format: u32,
        compression: u8,
        depth: u8,
    },

    #[error("{name} : {actual} octets d'image, {expected} attendus")]
    WrongSize {
        name: String,
        expected: usize,
        actual: usize,
    },
}

impl NativeTexture {
    /// Decodes the largest mipmap level. Only the formats met in Vice City
    /// are handled: DXT1, DXT3, 8-bit palettes and 32-bit colours.
    pub fn decode_rgba8(&self) -> Result<Rgba8Image, DecodeError> {
        let data = self.levels.first().ok_or_else(|| DecodeError::NoLevel {
            name: self.name.clone(),
        })?;
        let (width, height) = (usize::from(self.width), usize::from(self.height));
        let pixel_format = self.raster_format & raster_format::PIXEL_MASK;
        let check_size = |expected: usize| {
            if data.len() == expected {
                Ok(())
            } else {
                Err(DecodeError::WrongSize {
                    name: self.name.clone(),
                    expected,
                    actual: data.len(),
                })
            }
        };

        let mut pixels = vec![0; width * height * 4];
        match (self.compression, &self.palette, self.depth) {
            (1, None, _) => {
                check_size(texpresso::Format::Bc1.compressed_size(width, height))?;
                texpresso::Format::Bc1.decompress(data, width, height, &mut pixels);
                // DXT1 without alpha (565): the "transparent" code is black.
                if pixel_format != raster_format::FORMAT_1555 && !self.has_alpha {
                    force_opaque(&mut pixels);
                }
            }
            (3, None, _) => {
                check_size(texpresso::Format::Bc2.compressed_size(width, height))?;
                texpresso::Format::Bc2.decompress(data, width, height, &mut pixels);
            }
            (0, Some(palette), 8) => {
                check_size(width * height)?;
                for (pixel, &index) in pixels.as_chunks_mut::<4>().0.iter_mut().zip(data) {
                    // HYPOTHÈSE: palette entries are R, G, B, A (GTAMods
                    // wiki); to be confirmed visually on a loading screen.
                    *pixel = palette[usize::from(index)];
                }
                // With 888 palettes the fourth byte holds leftover values.
                if pixel_format != raster_format::FORMAT_8888 {
                    force_opaque(&mut pixels);
                }
            }
            (0, None, 32) if pixel_format == raster_format::FORMAT_888 => {
                check_size(width * height * 4)?;
                // HYPOTHÈSE: B, G, R, unused (GTAMods wiki); a single texture
                // of gta3.img uses it, not checked visually.
                let texels = data.as_chunks::<4>().0;
                for (pixel, bgrx) in pixels.as_chunks_mut::<4>().0.iter_mut().zip(texels) {
                    *pixel = [bgrx[2], bgrx[1], bgrx[0], 255];
                }
            }
            _ => {
                return Err(DecodeError::Unsupported {
                    name: self.name.clone(),
                    raster_format: self.raster_format,
                    compression: self.compression,
                    depth: self.depth,
                });
            }
        }
        Ok(Rgba8Image {
            width: u32::from(self.width),
            height: u32::from(self.height),
            pixels,
        })
    }
}

fn force_opaque(pixels: &mut [u8]) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }
}

#[cfg(test)]
mod tests {
    //! Synthetic TXD files built in code: no game data.

    use super::*;
    use crate::rw::test_util::*;

    struct Raster<'a> {
        name: &'a str,
        raster_format: u32,
        has_alpha: bool,
        size: (u16, u16),
        depth: u8,
        compression: u8,
        palette: Option<Vec<u8>>,
        levels: Vec<Vec<u8>>,
    }

    fn raster(raster: Raster<'_>) -> Vec<u8> {
        let mut name = [0u8; 32];
        name[..raster.name.len()].copy_from_slice(raster.name.as_bytes());
        let mut data = Payload::default()
            .u32(PLATFORM_D3D8)
            .u32(0x1106)
            .bytes(&name)
            .bytes(&[0; 32])
            .u32(raster.raster_format)
            .u32(u32::from(raster.has_alpha))
            .u16(raster.size.0)
            .u16(raster.size.1)
            .bytes(&[
                raster.depth,
                raster.levels.len() as u8,
                4,
                raster.compression,
            ]);
        if let Some(palette) = &raster.palette {
            data = data.bytes(palette);
        }
        for level in &raster.levels {
            data = data.u32(level.len() as u32).bytes(level);
        }
        container(
            id::RASTER,
            LIB_3_4_0_3,
            &[
                chunk(id::STRUCT, LIB_3_4_0_3, &data.0),
                container(id::EXTENSION, LIB_3_4_0_3, &[]),
            ],
        )
    }

    fn txd(rasters: Vec<Vec<u8>>) -> Vec<u8> {
        let mut children = vec![chunk(
            id::STRUCT,
            LIB_3_4_0_3,
            &(rasters.len() as u32).to_le_bytes(),
        )];
        children.extend(rasters);
        children.push(container(id::EXTENSION, LIB_3_4_0_3, &[]));
        let mut bytes = container(id::TEXTURE_DICTIONARY, LIB_3_4_0_3, &children);
        bytes.extend([0; 16]); // IMG sector padding
        bytes
    }

    /// One 4x4 DXT1 block: endpoints red (0xF800) and blue (0x001F), the
    /// first row uses code 0 (red), the others code 1 (blue).
    const DXT1_RED_BLUE: [u8; 8] = [0x00, 0xF8, 0x1F, 0x00, 0x00, 0x55, 0x55, 0x55];

    /// DXT1 block in three-colour mode (first endpoint <= second): code 3
    /// is transparent.
    const DXT1_TRANSPARENT: [u8; 8] = [0x1F, 0x00, 0x00, 0xF8, 0xFF, 0xFF, 0xFF, 0xFF];

    #[test]
    fn parses_the_header_and_mipmap_levels() {
        let bytes = txd(vec![raster(Raster {
            name: "player",
            raster_format: raster_format::FORMAT_565 | raster_format::MIPMAP,
            has_alpha: false,
            size: (4, 4),
            depth: 16,
            compression: 1,
            palette: None,
            levels: vec![DXT1_RED_BLUE.to_vec(), DXT1_RED_BLUE.to_vec()],
        })]);
        let dictionary = parse_txd(&bytes).unwrap();
        assert_eq!(dictionary.version, Version(0x3_4003));
        let texture = dictionary.find("PLAYER").unwrap();
        assert_eq!(texture.name, "player");
        assert_eq!(texture.mask_name, "");
        assert_eq!((texture.width, texture.height, texture.depth), (4, 4, 16));
        assert_eq!((texture.raster_type, texture.compression), (4, 1));
        assert_eq!(texture.sampler, 0x1106);
        assert_eq!(texture.levels.len(), 2);
    }

    #[test]
    fn decodes_dxt1() {
        let bytes = txd(vec![raster(Raster {
            name: "a",
            raster_format: raster_format::FORMAT_565,
            has_alpha: false,
            size: (4, 4),
            depth: 16,
            compression: 1,
            palette: None,
            levels: vec![DXT1_RED_BLUE.to_vec()],
        })]);
        let image = parse_txd(&bytes).unwrap().textures[0]
            .decode_rgba8()
            .unwrap();
        assert_eq!((image.width, image.height), (4, 4));
        assert_eq!(&image.pixels[..4], &[255, 0, 0, 255], "top row: red");
        assert_eq!(&image.pixels[16..20], &[0, 0, 255, 255], "second row: blue");
    }

    #[test]
    fn dxt1_alpha_is_kept_only_for_1555_textures() {
        let texture = |raster_format, has_alpha| {
            let bytes = txd(vec![raster(Raster {
                name: "a",
                raster_format,
                has_alpha,
                size: (4, 4),
                depth: 16,
                compression: 1,
                palette: None,
                levels: vec![DXT1_TRANSPARENT.to_vec()],
            })]);
            parse_txd(&bytes).unwrap().textures[0]
                .decode_rgba8()
                .unwrap()
        };
        assert_eq!(texture(raster_format::FORMAT_1555, true).pixels[3], 0);
        assert_eq!(texture(raster_format::FORMAT_565, false).pixels[3], 255);
    }

    #[test]
    fn decodes_dxt3_alpha() {
        // Explicit 4-bit alpha: first pixel 0xF (opaque), second 0x0.
        let mut block = vec![0x0F, 0, 0, 0, 0, 0, 0, 0];
        block.extend(DXT1_RED_BLUE);
        let bytes = txd(vec![raster(Raster {
            name: "a",
            raster_format: raster_format::FORMAT_4444,
            has_alpha: true,
            size: (4, 4),
            depth: 16,
            compression: 3,
            palette: None,
            levels: vec![block],
        })]);
        let image = parse_txd(&bytes).unwrap().textures[0]
            .decode_rgba8()
            .unwrap();
        assert_eq!(&image.pixels[..8], &[255, 0, 0, 255, 255, 0, 0, 0]);
    }

    #[test]
    fn decodes_8_bit_palettes() {
        let mut palette = vec![0u8; 1024];
        palette[4..8].copy_from_slice(&[10, 20, 30, 77]); // colour 1
        let texture = |raster_format| {
            let bytes = txd(vec![raster(Raster {
                name: "a",
                raster_format,
                has_alpha: raster_format & 0x0F00 == raster_format::FORMAT_8888,
                size: (4, 4),
                depth: 8,
                compression: 0,
                palette: Some(palette.clone()),
                levels: vec![vec![1; 16]],
            })]);
            parse_txd(&bytes).unwrap().textures[0]
                .decode_rgba8()
                .unwrap()
        };
        let opaque = texture(raster_format::PAL8 | raster_format::FORMAT_888);
        assert_eq!(&opaque.pixels[..4], &[10, 20, 30, 255]);
        let with_alpha = texture(raster_format::PAL8 | raster_format::FORMAT_8888);
        assert_eq!(&with_alpha.pixels[..4], &[10, 20, 30, 77]);
    }

    #[test]
    fn decodes_32_bit_bgr() {
        let bytes = txd(vec![raster(Raster {
            name: "a",
            raster_format: raster_format::FORMAT_888,
            has_alpha: false,
            size: (1, 1),
            depth: 32,
            compression: 0,
            palette: None,
            levels: vec![vec![30, 20, 10, 0]],
        })]);
        let image = parse_txd(&bytes).unwrap().textures[0]
            .decode_rgba8()
            .unwrap();
        assert_eq!(image.pixels, [10, 20, 30, 255]);
    }

    #[test]
    fn reports_unsupported_and_inconsistent_textures() {
        let texture = |raster_format, depth, levels| {
            let bytes = txd(vec![raster(Raster {
                name: "a",
                raster_format,
                has_alpha: false,
                size: (4, 4),
                depth,
                compression: 0,
                palette: None,
                levels,
            })]);
            parse_txd(&bytes).unwrap().textures[0].decode_rgba8()
        };
        assert!(matches!(
            texture(raster_format::FORMAT_565, 16, vec![vec![0; 32]]),
            Err(DecodeError::Unsupported { .. })
        ));
        assert!(matches!(
            texture(raster_format::FORMAT_888, 32, vec![vec![0; 10]]),
            Err(DecodeError::WrongSize {
                expected: 64,
                actual: 10,
                ..
            })
        ));
    }

    #[test]
    fn rejects_other_platforms() {
        let mut bytes = txd(vec![raster(Raster {
            name: "a",
            raster_format: raster_format::FORMAT_888,
            has_alpha: false,
            size: (1, 1),
            depth: 32,
            compression: 0,
            palette: None,
            levels: vec![vec![0; 4]],
        })]);
        // Platform ID of the first raster: 2 instead of 8.
        let platform_offset = 12 + 16 + 12 + 12;
        bytes[platform_offset] = 2;
        let err = parse_txd(&bytes).unwrap_err();
        assert!(err.to_string().contains("plateforme 2"), "{err}");
    }
}
