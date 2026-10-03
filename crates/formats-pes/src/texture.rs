//! PES 6 textures (signature `94 72 85 29`): a 128-byte header, a palette,
//! then 8-bit or 4-bit palette indices, laid out as on the PlayStation 2.
//!
//! Layout checked on the textures of the full PES 6 PC game, see
//! `docs/formats/pes-texture.md`.

use crate::content::TEXTURE_MAGIC;

/// Size of the header before the palette of 8-bit textures.
pub const HEADER_SIZE: usize = 128;

/// Pixel storage of a texture, named after the PlayStation 2 formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// PSMT8 (`0x13`): one byte per pixel, 256-colour palette.
    Indexed8,
    /// PSMT4 (`0x14`): half a byte per pixel, 16-colour palette.
    Indexed4,
}

impl PixelFormat {
    fn palette_len(self) -> usize {
        match self {
            PixelFormat::Indexed8 => 256,
            PixelFormat::Indexed4 => 16,
        }
    }

    fn pixel_bytes(self, width: usize, height: usize) -> usize {
        match self {
            PixelFormat::Indexed8 => width * height,
            PixelFormat::Indexed4 => (width * height).div_ceil(2),
        }
    }
}

/// The fields of the header that decoding needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureHeader {
    pub width: u16,
    pub height: u16,
    pub format: PixelFormat,
    /// Position of the palette in the file.
    pub palette_offset: usize,
    /// Position of the pixels in the file.
    pub pixel_offset: usize,
    /// Size given by the header: the file's size, give or take the padding
    /// of sub-files.
    pub file_size: usize,
    /// Number by which models name the texture (offset 12), see
    /// `model::PesModel::texture_ids`.
    pub id: u32,
    /// 8-bit pixels stored in the order of a 32-bit image of half the size
    /// (see [`unswizzle8`]).
    pub swizzled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TextureError {
    #[error("pas une texture PES (signature {0:02x?})")]
    BadMagic([u8; 4]),

    #[error("en-tête de texture tronqué ({0} octets)")]
    Truncated(usize),

    #[error("format de pixels inconnu : {0:#04x}")]
    UnknownFormat(u8),

    #[error("texture sans pixels : palette seule")]
    PaletteOnly,

    #[error("texture sans palette : sa palette est dans un autre fichier")]
    ExternalPalette,

    #[error(
        "texture {width} × {height} rangée comme une image 32 bits ({stored_width} × {stored_height}) : non prise en charge"
    )]
    Swizzled {
        width: u16,
        height: u16,
        stored_width: u16,
        stored_height: u16,
    },

    #[error("données trop courtes : {needed} octets attendus, {len} présents")]
    TooShort { needed: usize, len: usize },
}

impl TextureHeader {
    pub fn parse(bytes: &[u8]) -> Result<Self, TextureError> {
        let header = bytes
            .get(..HEADER_SIZE)
            .ok_or(TextureError::Truncated(bytes.len()))?;
        let u16_at = |at: usize| u16::from_le_bytes([header[at], header[at + 1]]);
        let magic: [u8; 4] = header[..4].try_into().unwrap();
        if magic != TEXTURE_MAGIC {
            return Err(TextureError::BadMagic(magic));
        }
        let format = match header[25] {
            0x13 => PixelFormat::Indexed8,
            0x14 => PixelFormat::Indexed4,
            other => return Err(TextureError::UnknownFormat(other)),
        };
        let (width, height) = (u16_at(20), u16_at(22));
        if width == 0 || height == 0 {
            return Err(TextureError::PaletteOnly);
        }
        // HYPOTHÈSE: offsets 40 and 42 give the size the pixels were
        // uploaded with. Equal to the size for nearly all textures; half of
        // it in both directions for some of them (the balls among others),
        // whose pixels are then laid out as a 32-bit image (PlayStation 2
        // swizzle): undone for 8-bit pixels, not handled yet for 4-bit ones.
        let (stored_width, stored_height) = (u16_at(40), u16_at(42));
        let swizzled = (stored_width, stored_height) == (width / 2, height / 2);
        if swizzled && format == PixelFormat::Indexed4 {
            return Err(TextureError::Swizzled {
                width,
                height,
                stored_width,
                stored_height,
            });
        }
        Ok(Self {
            width,
            height,
            format,
            palette_offset: usize::from(u16_at(18)),
            pixel_offset: usize::from(u16_at(16)),
            file_size: u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize,
            id: u32::from_le_bytes(header[12..16].try_into().unwrap()),
            swizzled,
        })
    }
}

/// A decoded texture: 4 bytes per pixel (R, G, B, A), rows from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedTexture {
    /// See [`TextureHeader::id`].
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
}

/// Position in the stored palette of colour `index`, for 256-colour
/// palettes: the PlayStation 2 stores them in blocks of 8 colours where
/// the second and third blocks of every 32 are swapped (bits 3 and 4 of the
/// index exchanged).
// HYPOTHÈSE: checked by eye on a face texture, and the reordered palettes
// of the game have smoother gradients than the stored order.
fn stored_palette_index(index: usize) -> usize {
    (index & !0x18) | ((index & 0x08) << 1) | ((index & 0x10) >> 1)
}

/// Puts back in rows the 8-bit pixels of a texture uploaded to the
/// PlayStation 2 as a 32-bit image (GS memory layout: blocks of 16 × 16
/// pixels, columns of 16 × 4 pixels in which each 32-bit word holds four
/// pixels of two rows). The usual formula of the public PlayStation 2
/// texture tools.
// HYPOTHÈSE: checked by eye on the balls (0_text:1: hexagons and a
// pentagon), whose pixels are a striped mess when read in order.
pub fn unswizzle8(stored: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut rows = vec![0; width * height];
    for y in 0..height {
        for x in 0..width {
            let block = (y & !0xf) * width + (x & !0xf) * 2;
            let swap = (((y + 2) >> 2) & 1) * 4;
            let row_in_column = (((y & !3) >> 1) + (y & 1)) & 7;
            let column = row_in_column * width * 2 + ((x + swap) & 7) * 4;
            let byte = ((y >> 1) & 1) + ((x >> 2) & 2);
            rows[y * width + x] = stored.get(block + column + byte).copied().unwrap_or(0);
        }
    }
    rows
}

/// Decodes a texture file into RGBA pixels.
pub fn decode(bytes: &[u8]) -> Result<DecodedTexture, TextureError> {
    let header = TextureHeader::parse(bytes)?;
    let (width, height) = (usize::from(header.width), usize::from(header.height));
    let colours = header.format.palette_len();
    let palette_end = header.palette_offset + 4 * colours;
    let pixels_end = header.pixel_offset + header.format.pixel_bytes(width, height);
    // HYPOTHÈSE: when the palette would start where the pixels start, the
    // texture has none and takes one from another file (face textures of
    // the editor; the "palettes" slots of the community map).
    if header.palette_offset == header.pixel_offset {
        return Err(TextureError::ExternalPalette);
    }
    // HYPOTHÈSE: a file that stops before its pixels (128 or 1152 bytes)
    // is a palette for the pixels of the previous texture of its container,
    // which has the same size (colour variants).
    if bytes.len() <= header.pixel_offset && bytes.len() >= palette_end.min(HEADER_SIZE) {
        return Err(TextureError::PaletteOnly);
    }
    let needed = palette_end.max(pixels_end);
    if bytes.len() < needed {
        return Err(TextureError::TooShort {
            needed,
            len: bytes.len(),
        });
    }

    let stored = &bytes[header.palette_offset..palette_end];
    // HYPOTHÈSE: alpha follows the PlayStation 2 convention (0x80 = opaque)
    // when no colour of the palette goes above 0x80, which is the case for
    // 7550 of the 7552 palettes of the game; the others use the full range.
    let half_range = stored
        .as_chunks::<4>()
        .0
        .iter()
        .all(|colour| colour[3] <= 0x80);
    let palette: Vec<[u8; 4]> = (0..colours)
        .map(|index| {
            let at = match header.format {
                PixelFormat::Indexed8 => stored_palette_index(index),
                PixelFormat::Indexed4 => index,
            };
            let [r, g, b, a]: [u8; 4] = stored[4 * at..4 * at + 4].try_into().unwrap();
            let alpha = if half_range { a.saturating_mul(2) } else { a };
            [r, g, b, alpha]
        })
        .collect();

    let stored_pixels = &bytes[header.pixel_offset..pixels_end];
    let unswizzled;
    let pixels = if header.swizzled {
        unswizzled = unswizzle8(stored_pixels, width, height);
        &unswizzled[..]
    } else {
        stored_pixels
    };
    let mut rgba8 = Vec::with_capacity(width * height * 4);
    for index in 0..width * height {
        let entry = match header.format {
            PixelFormat::Indexed8 => pixels[index],
            // HYPOTHÈSE: the first pixel is in the low half of the byte.
            PixelFormat::Indexed4 => (pixels[index / 2] >> (4 * (index % 2))) & 0x0f,
        };
        rgba8.extend_from_slice(&palette[usize::from(entry)]);
    }
    Ok(DecodedTexture {
        id: header.id,
        width: u32::from(header.width),
        height: u32::from(header.height),
        rgba8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a texture file: header, palette, pixels.
    fn texture(format: u8, width: u16, height: u16, palette: &[[u8; 4]], pixels: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0u8; HEADER_SIZE];
        bytes[..4].copy_from_slice(&TEXTURE_MAGIC);
        let palette_offset = if format == 0x13 { HEADER_SIZE } else { 64 };
        let pixel_offset = if format == 0x13 {
            HEADER_SIZE + 1024
        } else {
            HEADER_SIZE
        };
        bytes[16..18].copy_from_slice(&(pixel_offset as u16).to_le_bytes());
        bytes[18..20].copy_from_slice(&(palette_offset as u16).to_le_bytes());
        bytes[20..22].copy_from_slice(&width.to_le_bytes());
        bytes[22..24].copy_from_slice(&height.to_le_bytes());
        bytes[25] = format;
        bytes[40..42].copy_from_slice(&width.to_le_bytes());
        bytes[42..44].copy_from_slice(&height.to_le_bytes());
        bytes.resize(pixel_offset, 0);
        for (index, colour) in palette.iter().enumerate() {
            let at = palette_offset + 4 * index;
            bytes[at..at + 4].copy_from_slice(colour);
        }
        bytes.extend_from_slice(pixels);
        let size = bytes.len() as u32;
        bytes[8..12].copy_from_slice(&size.to_le_bytes());
        bytes
    }

    #[test]
    fn decodes_8_bit_textures_with_a_reordered_palette() {
        let mut palette = vec![[0u8; 4]; 256];
        // Colour 8 is stored in slot 16, colour 16 in slot 8.
        palette[16] = [10, 20, 30, 0x80];
        palette[8] = [40, 50, 60, 0x40];
        palette[1] = [1, 2, 3, 0x80];
        let bytes = texture(0x13, 2, 2, &palette, &[8, 16, 1, 0]);
        let decoded = decode(&bytes).unwrap();
        assert_eq!((decoded.width, decoded.height), (2, 2));
        assert_eq!(
            decoded.rgba8,
            [10, 20, 30, 255, 40, 50, 60, 128, 1, 2, 3, 255, 0, 0, 0, 0]
        );
    }

    #[test]
    fn decodes_4_bit_textures_low_half_first() {
        let mut palette = vec![[0u8; 4]; 16];
        palette[1] = [255, 0, 0, 0x80];
        palette[2] = [0, 255, 0, 0x80];
        let bytes = texture(0x14, 2, 1, &palette, &[0x21]);
        assert_eq!(
            decode(&bytes).unwrap().rgba8,
            [255, 0, 0, 255, 0, 255, 0, 255]
        );
    }

    #[test]
    fn keeps_full_range_alpha() {
        let mut palette = vec![[0u8; 4]; 16];
        palette[0] = [9, 9, 9, 0xf0];
        palette[1] = [9, 9, 9, 0x40];
        let bytes = texture(0x14, 2, 1, &palette, &[0x10]);
        let rgba = decode(&bytes).unwrap().rgba8;
        assert_eq!((rgba[3], rgba[7]), (0xf0, 0x40));
    }

    #[test]
    fn unswizzling_moves_every_pixel_once() {
        // 256 different bytes in one 16 × 16 block: each comes out once,
        // in another order.
        let stored: Vec<u8> = (0..=255).collect();
        let rows = unswizzle8(&stored, 16, 16);
        let mut sorted = rows.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, stored);
        assert_ne!(rows, stored);
        // The first row of a block takes bytes from the first two words of
        // each 32-bit column.
        assert_eq!(&rows[..4], &[0, 4, 8, 12]);
    }

    #[test]
    fn rejects_unsupported_textures() {
        let palette = vec![[0u8; 4]; 256];
        // Swizzled 4-bit pixels are not handled; 8-bit ones are.
        let mut swizzled = texture(0x14, 4, 4, &palette[..16], &[0; 8]);
        swizzled[40..42].copy_from_slice(&2u16.to_le_bytes());
        swizzled[42..44].copy_from_slice(&2u16.to_le_bytes());
        assert!(matches!(
            decode(&swizzled),
            Err(TextureError::Swizzled { .. })
        ));
        let mut swizzled = texture(0x13, 4, 4, &palette, &[0; 16]);
        swizzled[40..42].copy_from_slice(&2u16.to_le_bytes());
        swizzled[42..44].copy_from_slice(&2u16.to_le_bytes());
        assert!(decode(&swizzled).is_ok());

        let empty = texture(0x13, 0, 0, &palette, &[]);
        assert_eq!(decode(&empty), Err(TextureError::PaletteOnly));

        let mut no_palette = texture(0x13, 2, 2, &palette, &[0; 4]);
        no_palette.copy_within(16..18, 18);
        assert_eq!(decode(&no_palette), Err(TextureError::ExternalPalette));

        let header_and_palette = texture(0x13, 4, 4, &palette, &[]);
        assert_eq!(decode(&header_and_palette), Err(TextureError::PaletteOnly));

        let short = texture(0x13, 4, 4, &palette, &[0; 3]);
        assert!(matches!(decode(&short), Err(TextureError::TooShort { .. })));

        let mut other = texture(0x13, 1, 1, &palette, &[0]);
        other[25] = 0x00;
        assert_eq!(decode(&other), Err(TextureError::UnknownFormat(0)));
    }
}
