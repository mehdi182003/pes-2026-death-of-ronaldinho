//! Writes 32-bit BMP files (captures of PES's image).

/// A top-down 32-bit BMP of `width × height` pixels given as B, G, R, A rows.
pub fn encode_bgra(width: u32, height: u32, bgra: &[u8]) -> Vec<u8> {
    let pixels = width as usize * height as usize * 4;
    assert_eq!(bgra.len(), pixels, "taille des pixels");
    let header = 14 + 40;
    let mut out = Vec::with_capacity(header + pixels);
    // BITMAPFILEHEADER
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((header + pixels) as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(header as u32).to_le_bytes());
    // BITMAPINFOHEADER, negative height: rows from the top.
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(width as i32).to_le_bytes());
    out.extend_from_slice(&(-(height as i32)).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&(pixels as u32).to_le_bytes());
    out.extend_from_slice(&[0; 16]);
    out.extend_from_slice(bgra);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_and_pixels() {
        let bmp = encode_bgra(2, 1, &[1, 2, 3, 255, 4, 5, 6, 255]);
        assert_eq!(&bmp[..2], b"BM");
        assert_eq!(u32::from_le_bytes(bmp[2..6].try_into().unwrap()), 62);
        assert_eq!(i32::from_le_bytes(bmp[22..26].try_into().unwrap()), -1);
        assert_eq!(&bmp[54..], &[1, 2, 3, 255, 4, 5, 6, 255]);
    }
}
