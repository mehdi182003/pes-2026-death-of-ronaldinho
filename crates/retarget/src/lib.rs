//! Skeleton retargeting between Vice City and PES 6 rigs: bone mapping,
//! scale and axis conventions.

use std::f32::consts::FRAC_1_SQRT_2;

/// Rotation from Vice City's world to a Y-up world such as Bevy's, as a
/// quaternion (x, y, z, w): -90° around X.
///
/// Vice City is Z-up, and characters move towards +Y (the root of
/// `run_player` advances along +Y). After this rotation, up is +Y and their
/// forward is -Z, Bevy's forward. Both worlds use metres.
pub const VICE_CITY_TO_Y_UP: [f32; 4] = [-FRAC_1_SQRT_2, 0.0, 0.0, FRAC_1_SQRT_2];

/// Units of PES 6 models per metre. PES 6 models are already Y-up, feet at
/// Y = 0 (the referee's model goes from 0 to 756 units, head included).
// HYPOTHÈSE: chosen so that this referee is 1.80 m tall; to be checked by
// eye next to Tommy. The faces (about 90 units) then measure about 21 cm.
pub const PES6_UNITS_PER_METRE: f32 = 420.0;

#[cfg(test)]
mod tests {
    use super::*;

    /// Rotates `v` by the unit quaternion `q`.
    fn rotate([x, y, z, w]: [f32; 4], v: [f32; 3]) -> [f32; 3] {
        // v' = v + 2w (u × v) + 2 u × (u × v), with u = (x, y, z).
        let u = [x, y, z];
        let cross = |a: [f32; 3], b: [f32; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let t = cross(u, v).map(|c| 2.0 * c);
        let ut = cross(u, t);
        std::array::from_fn(|i| v[i] + w * t[i] + ut[i])
    }

    fn assert_close(a: [f32; 3], b: [f32; 3]) {
        assert!(
            a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6),
            "{a:?} != {b:?}"
        );
    }

    #[test]
    fn vice_city_up_becomes_y_and_forward_becomes_minus_z() {
        assert_close(rotate(VICE_CITY_TO_Y_UP, [0.0, 0.0, 1.0]), [0.0, 1.0, 0.0]);
        assert_close(rotate(VICE_CITY_TO_Y_UP, [0.0, 1.0, 0.0]), [0.0, 0.0, -1.0]);
        assert_close(rotate(VICE_CITY_TO_Y_UP, [1.0, 0.0, 0.0]), [1.0, 0.0, 0.0]);
    }
}
