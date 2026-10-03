//! Screen-space overlay geometry: coloured quads and a 5×7 bitmap font,
//! drawn with pre-transformed vertices (`D3DFVF_XYZRHW | D3DFVF_DIFFUSE`).

/// One pre-transformed vertex, laid out as Direct3D 8 expects for
/// `D3DFVF_XYZRHW | D3DFVF_DIFFUSE` (20 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rhw: f32,
    /// ARGB.
    pub color: u32,
}

/// `D3DFVF_XYZRHW | D3DFVF_DIFFUSE`.
pub const FVF: u32 = 0x0004 | 0x0040;

/// Appends an axis-aligned rectangle as two triangles (triangle list).
pub fn push_rect(out: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32, color: u32) {
    // Half-pixel offset: Direct3D 8 maps pixel centres to integer coordinates.
    let (x0, y0, x1, y1) = (x - 0.5, y - 0.5, x + w - 0.5, y + h - 0.5);
    let v = |x, y| Vertex {
        x,
        y,
        z: 0.0,
        rhw: 1.0,
        color,
    };
    out.extend([
        v(x0, y0),
        v(x1, y0),
        v(x0, y1),
        v(x1, y0),
        v(x1, y1),
        v(x0, y1),
    ]);
}

/// Rows of a 5×7 glyph, top first, bit 4 = leftmost column.
fn glyph(c: char) -> Option<[u8; 7]> {
    Some(match c.to_ascii_uppercase() {
        'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'C' => [0x0e, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0e],
        'F' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
        'H' => [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'O' => [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        '0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        '1' => [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
        '2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        '3' => [0x1f, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0e],
        '4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        '5' => [0x1f, 0x10, 0x1e, 0x01, 0x01, 0x11, 0x0e],
        '6' => [0x06, 0x08, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        '7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        '9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x02, 0x0c],
        '.' => [0, 0, 0, 0, 0, 0x0c, 0x0c],
        ' ' => [0; 7],
        _ => return None,
    })
}

/// Width in pixels of `text` drawn at `scale` (6 columns per character,
/// including spacing).
pub fn text_width(text: &str, scale: f32) -> f32 {
    text.chars().count() as f32 * 6.0 * scale - scale
}

/// Appends `text` with its top-left corner at (x, y). Unknown characters are
/// drawn as a filled block so they are noticed.
pub fn push_text(out: &mut Vec<Vertex>, text: &str, x: f32, y: f32, scale: f32, color: u32) {
    for (i, c) in text.chars().enumerate() {
        let left = x + i as f32 * 6.0 * scale;
        let rows = glyph(c).unwrap_or([0x1f; 7]);
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..5 {
                if bits & (0x10 >> col) != 0 {
                    let px = left + col as f32 * scale;
                    let py = y + row as f32 * scale;
                    push_rect(out, px, py, scale, scale, color);
                }
            }
        }
    }
}

/// The M2 banner: proves the mod draws inside PES's own frame.
pub fn banner(frame: u64) -> Vec<Vertex> {
    const TEXT: &str = "CHAOS FC";
    let scale = 3.0;
    let (x, y, pad) = (16.0, 16.0, 6.0);
    let mut out = Vec::new();
    push_rect(
        &mut out,
        x - pad,
        y - pad,
        text_width(TEXT, scale) + 2.0 * pad,
        7.0 * scale + 2.0 * pad,
        0xb0_00_00_00,
    );
    // A red bar under the text, sliding with the frame count: shows the
    // overlay is redrawn every frame, not frozen.
    let bar = text_width(TEXT, scale);
    let t = (frame % 120) as f32 / 120.0;
    push_rect(
        &mut out,
        x + bar * t * 0.75,
        y + 7.0 * scale + 2.0,
        bar * 0.25,
        2.0,
        0xff_e0_20_20,
    );
    push_text(&mut out, TEXT, x, y, scale, 0xff_ff_ff_ff);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_matches_the_fvf_stride() {
        assert_eq!(std::mem::size_of::<Vertex>(), 20);
    }

    #[test]
    fn rect_is_two_triangles() {
        let mut out = Vec::new();
        push_rect(&mut out, 10.0, 20.0, 4.0, 2.0, 0xffffffff);
        assert_eq!(out.len(), 6);
        assert_eq!((out[0].x, out[0].y), (9.5, 19.5));
        assert_eq!((out[4].x, out[4].y), (13.5, 21.5));
    }

    #[test]
    fn text_draws_one_quad_per_lit_pixel() {
        let mut out = Vec::new();
        push_text(&mut out, "1", 0.0, 0.0, 1.0, 0xffffffff);
        let lit: u32 = glyph('1').unwrap().iter().map(|r| r.count_ones()).sum();
        assert_eq!(out.len(), lit as usize * 6);
    }

    #[test]
    fn every_banner_character_has_a_glyph() {
        assert!("CHAOS FC 0123456789.".chars().all(|c| glyph(c).is_some()));
    }

    #[test]
    fn banner_moves_with_the_frame() {
        assert_ne!(banner(0), banner(60));
        assert_eq!(banner(0), banner(120));
    }
}
