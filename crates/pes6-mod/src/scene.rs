//! Finds PES's 3D camera in a frame.
//!
//! PES 6 uses the fixed pipeline: it sets `D3DTS_VIEW` and `D3DTS_PROJECTION`
//! with `SetTransform` (seen in the M2a log), but a frame can use several
//! cameras (the pitch, then the score and radar in 2D). The camera that draws
//! the most primitives is taken as the one of the pitch.

pub type Matrix = [f32; 16];

pub const IDENTITY: Matrix = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

/// A camera used in the frame, with the number of primitives drawn through it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub view: Matrix,
    pub projection: Matrix,
    pub primitives: u64,
}

/// Cameras of the frame being drawn.
#[derive(Debug, Default)]
pub struct FrameCameras {
    view: Option<Matrix>,
    projection: Option<Matrix>,
    cameras: Vec<Camera>,
}

impl FrameCameras {
    pub const fn new() -> Self {
        Self {
            view: None,
            projection: None,
            cameras: Vec::new(),
        }
    }

    pub fn set_view(&mut self, view: Matrix) {
        self.view = Some(view);
    }

    pub fn set_projection(&mut self, projection: Matrix) {
        self.projection = Some(projection);
    }

    /// Counts primitives drawn with the current camera. Draws made before
    /// both matrices are known are ignored.
    pub fn draw(&mut self, primitives: u32) {
        let (Some(view), Some(projection)) = (self.view, self.projection) else {
            return;
        };
        match self
            .cameras
            .iter_mut()
            .find(|c| c.view == view && c.projection == projection)
        {
            Some(camera) => camera.primitives += u64::from(primitives),
            None => self.cameras.push(Camera {
                view,
                projection,
                primitives: u64::from(primitives),
            }),
        }
    }

    /// Ends the frame: returns the camera that drew the most, and the number
    /// of distinct cameras. The current matrices stay set, as on the device.
    pub fn finish(&mut self) -> (Option<Camera>, usize) {
        let count = self.cameras.len();
        let main = self
            .cameras
            .drain(..)
            .max_by_key(|camera| camera.primitives);
        (main, count)
    }
}

/// Row-vector product used by Direct3D: `a` then `b`.
pub fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    let mut out = [0.0; 16];
    for row in 0..4 {
        for col in 0..4 {
            out[row * 4 + col] = (0..4).map(|k| a[row * 4 + k] * b[k * 4 + col]).sum();
        }
    }
    out
}

/// Projects a world point to normalised device coordinates (x, y in -1..1,
/// z in 0..1), or `None` behind the camera.
pub fn project(point: [f32; 3], view: &Matrix, projection: &Matrix) -> Option<[f32; 3]> {
    let m = multiply(view, projection);
    let [x, y, z] = point;
    let clip: Vec<f32> = (0..4)
        .map(|col| x * m[col] + y * m[4 + col] + z * m[8 + col] + m[12 + col])
        .collect();
    (clip[3] > 0.0).then(|| [clip[0] / clip[3], clip[1] / clip[3], clip[2] / clip[3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The view and projection logged by PES 6 in M2a.
    const PES_VIEW: Matrix = [
        -0.99999994,
        0.0,
        0.0,
        0.0,
        0.0,
        0.4472136,
        0.89442724,
        0.0,
        0.0,
        0.8944272,
        -0.44721362,
        0.0,
        0.0,
        0.0,
        2236.068,
        1.0,
    ];
    const PES_PROJECTION: Matrix = [
        5.5859375, 0.0, 0.0, 0.0, 0.0, -7.447917, 0.0, 0.0, 0.0, 0.0, 1.0031348, 1.0, 0.0, 0.0,
        -50.15674, 0.0,
    ];

    #[test]
    fn the_busiest_camera_wins() {
        let mut frame = FrameCameras::default();
        frame.draw(100); // before any matrix: ignored
        frame.set_projection(PES_PROJECTION);
        frame.set_view(PES_VIEW);
        frame.draw(500);
        frame.draw(300);
        frame.set_view(IDENTITY); // 2D overlay
        frame.draw(50);
        let (main, count) = frame.finish();
        assert_eq!(count, 2);
        let main = main.unwrap();
        assert_eq!(main.view, PES_VIEW);
        assert_eq!(main.primitives, 800);
    }

    #[test]
    fn finish_starts_a_new_frame_with_the_same_matrices() {
        let mut frame = FrameCameras::default();
        frame.set_view(PES_VIEW);
        frame.set_projection(PES_PROJECTION);
        frame.draw(10);
        frame.finish();
        assert_eq!(frame.finish(), (None, 0));
        frame.draw(5);
        assert_eq!(frame.finish().0.unwrap().primitives, 5);
    }

    #[test]
    fn identity_is_neutral() {
        assert_eq!(multiply(&PES_VIEW, &IDENTITY), PES_VIEW);
        assert_eq!(multiply(&IDENTITY, &PES_VIEW), PES_VIEW);
    }

    #[test]
    fn the_world_origin_is_in_front_of_the_pes_camera() {
        // The camera of the M2a log looks at the origin from 2236 units away.
        let ndc = project([0.0, 0.0, 0.0], &PES_VIEW, &PES_PROJECTION).unwrap();
        assert!(ndc[0].abs() < 1e-3 && ndc[1].abs() < 1e-3, "{ndc:?}");
        assert!((0.0..1.0).contains(&ndc[2]), "{ndc:?}");
    }
}
