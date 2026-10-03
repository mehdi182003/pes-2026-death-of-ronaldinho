//! A test marker drawn in PES's world (M2b): a cross on the ground and a post
//! on each side of the ground plane, so one screenshot shows where the origin
//! is, how big a unit is, and which way is up.

/// A world-space vertex for `D3DFVF_XYZ | D3DFVF_DIFFUSE` (16 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// ARGB.
    pub color: u32,
}

/// `D3DFVF_XYZ | D3DFVF_DIFFUSE`.
pub const FVF: u32 = 0x0002 | 0x0040;

pub const CROSS_COLOR: u32 = 0xff_ff_e0_00;
/// Post on the +Y side of the ground.
pub const PLUS_Y_COLOR: u32 = 0xff_ff_20_20;
/// Post on the −Y side of the ground.
pub const MINUS_Y_COLOR: u32 = 0xff_20_60_ff;

// HYPOTHÈSE: PES's in-match world uses the stadium scale, 51.3 units per
// metre (docs/formats/pes-stadium.md). The marker is sized from it: a 10 m
// cross, 3 m posts, 0.3 m thick. Wrong units only make it bigger or smaller.
pub const UNITS_PER_METRE: f32 = 51.3;

fn quad(out: &mut Vec<WorldVertex>, corners: [[f32; 3]; 4], color: u32) {
    let v = |[x, y, z]: [f32; 3]| WorldVertex { x, y, z, color };
    let [a, b, c, d] = corners;
    out.extend([v(a), v(b), v(c), v(a), v(c), v(d)]);
}

/// Axis-aligned box between two corners, as 12 triangles.
fn cuboid(out: &mut Vec<WorldVertex>, min: [f32; 3], max: [f32; 3], color: u32) {
    let [x0, y0, z0] = min;
    let [x1, y1, z1] = max;
    let faces = [
        [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0]],
        [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
        [[x0, y0, z0], [x0, y1, z0], [x0, y1, z1], [x0, y0, z1]],
        [[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]],
        [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
        [[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]],
    ];
    for face in faces {
        quad(out, face, color);
    }
}

/// The marker centred on `centre`, as a triangle list.
pub fn marker(centre: [f32; 3]) -> Vec<WorldVertex> {
    let m = UNITS_PER_METRE;
    let (arm, thick, post) = (5.0 * m, 0.3 * m, 3.0 * m);
    let [cx, cy, cz] = centre;
    let h = thick / 2.0;
    let mut out = Vec::new();
    // Cross on the ground (X and Z arms), as flat boxes.
    cuboid(
        &mut out,
        [cx - arm, cy - h / 4.0, cz - h],
        [cx + arm, cy + h / 4.0, cz + h],
        CROSS_COLOR,
    );
    cuboid(
        &mut out,
        [cx - h, cy - h / 4.0, cz - arm],
        [cx + h, cy + h / 4.0, cz + arm],
        CROSS_COLOR,
    );
    // Posts.
    cuboid(
        &mut out,
        [cx - h, cy, cz - h],
        [cx + h, cy + post, cz + h],
        PLUS_Y_COLOR,
    );
    cuboid(
        &mut out,
        [cx - h, cy - post, cz - h],
        [cx + h, cy, cz + h],
        MINUS_Y_COLOR,
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_matches_the_fvf_stride() {
        assert_eq!(std::mem::size_of::<WorldVertex>(), 16);
    }

    #[test]
    fn marker_is_four_boxes_of_triangles() {
        let m = marker([0.0; 3]);
        assert_eq!(m.len(), 4 * 6 * 6);
        assert_eq!(m.len() % 3, 0);
    }

    #[test]
    fn posts_point_both_ways() {
        let m = marker([10.0, 0.0, 0.0]);
        let top = m
            .iter()
            .filter(|v| v.color == PLUS_Y_COLOR)
            .map(|v| v.y)
            .fold(f32::MIN, f32::max);
        let bottom = m
            .iter()
            .filter(|v| v.color == MINUS_Y_COLOR)
            .map(|v| v.y)
            .fold(f32::MAX, f32::min);
        assert!(top > 100.0 && bottom < -100.0);
        assert!(
            m.iter()
                .all(|v| (v.x - 10.0).abs() <= 5.0 * UNITS_PER_METRE + 1e-3)
        );
    }
}
