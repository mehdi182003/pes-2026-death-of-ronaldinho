//! The ball: a Rapier rigid body with the size and mass of a real one, the
//! air drag, the curl of a spinning ball and the slowing down of a ball
//! rolling on grass, which Rapier does not model.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

/// Size 5 ball (Laws of the Game): 68 to 70 cm round, 410 to 450 g.
pub const RADIUS: f32 = 0.11;
pub const MASS: f32 = 0.43;
const GRAVITY: f32 = 9.81;

/// Air density (kg/m³) and drag coefficient of a ball in flight.
const AIR_DENSITY: f32 = 1.2;
const DRAG_COEFFICIENT: f32 = 0.25;
/// Lift from spin (Magnus effect): F = MAGNUS · (ω × v), in kg.
// HYPOTHÈSE: ½ ρ A r with a lift coefficient of about 1, so that a shot at
// 25 m/s spinning at 10 turns per second curls by a few metres over 20 m;
// to be tuned by eye.
const MAGNUS: f32 = 0.5 * AIR_DENSITY * std::f32::consts::PI * RADIUS * RADIUS * RADIUS;
/// Slowing down of a ball rolling on grass, as a fraction of its weight.
// HYPOTHÈSE: 1 m/s², with the drag of the air a pass rolling at 10 m/s
// stops after about 32 m; to be tuned by eye.
const ROLLING_RESISTANCE: f32 = 0.1;

/// Marks the ball.
#[derive(Component)]
pub struct Ball;

/// The physics of the ball, to add to its entity with its transform.
pub fn body() -> impl Bundle {
    (
        Ball,
        RigidBody::Dynamic,
        Collider::ball(RADIUS),
        ColliderMassProperties::Mass(MASS),
        // Grass gives back a bit more than half the speed of a falling ball.
        Restitution {
            coefficient: 0.65,
            combine_rule: CoefficientCombineRule::Average,
        },
        Friction::coefficient(0.6),
        // Shots go through thin posts otherwise.
        Ccd::enabled(),
        Velocity::zero(),
        ExternalForce::default(),
        Damping {
            linear_damping: 0.0,
            angular_damping: 0.2,
        },
        // Never falls asleep: a player can touch it at any time.
        Sleeping::disabled(),
    )
}

/// Whether the ball touches the ground, its centre at `height`.
pub fn on_ground(height: f32) -> bool {
    height < RADIUS + 0.02
}

/// Air drag: against the velocity, growing with its square.
pub fn drag(velocity: Vec3) -> Vec3 {
    let area = std::f32::consts::PI * RADIUS * RADIUS;
    -0.5 * AIR_DENSITY * DRAG_COEFFICIENT * area * velocity.length() * velocity
}

/// Magnus effect: a spinning ball curls towards ω × v.
pub fn magnus(angular_velocity: Vec3, velocity: Vec3) -> Vec3 {
    MAGNUS * angular_velocity.cross(velocity)
}

/// Rolling resistance on the ground: against the horizontal velocity, of
/// constant strength, and never more than what stops the ball within
/// `dt` seconds.
pub fn rolling(velocity: Vec3, dt: f32) -> Vec3 {
    let horizontal = Vec3::new(velocity.x, 0.0, velocity.z);
    let speed = horizontal.length();
    if speed < 1e-4 {
        return Vec3::ZERO;
    }
    let strength = (ROLLING_RESISTANCE * MASS * GRAVITY).min(MASS * speed / dt.max(1e-4));
    -horizontal / speed * strength
}

/// Sets the forces of the air and the grass on the ball for this frame.
pub fn apply_forces(
    time: Res<Time>,
    mut balls: Query<(&Transform, &Velocity, &mut ExternalForce), With<Ball>>,
) {
    let dt = time.delta_secs();
    for (transform, velocity, mut force) in &mut balls {
        let mut total = drag(velocity.linear) + magnus(velocity.angular, velocity.linear);
        if on_ground(transform.translation.y) {
            total += rolling(velocity.linear, dt);
        }
        force.force = total;
        force.torque = Vec3::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_slows_a_shot_by_a_few_metres_per_second_squared() {
        let deceleration = drag(Vec3::X * 30.0).length() / MASS;
        assert!((5.0..20.0).contains(&deceleration), "{deceleration}");
        assert!(drag(Vec3::X * 30.0).x < 0.0);
    }

    #[test]
    fn topspin_dips_and_sidespin_curls() {
        // Rolling forwards along +X: spin around -Z (top of the ball moving
        // forwards) pushes it down.
        let forward = Vec3::X * 20.0;
        assert!(magnus(Vec3::NEG_Z * 60.0, forward).y < 0.0);
        // Spin around +Y curls it towards -Z... and the opposite spin the
        // other way.
        assert!(magnus(Vec3::Y * 60.0, forward).z < 0.0);
        assert!(magnus(Vec3::NEG_Y * 60.0, forward).z > 0.0);
    }

    #[test]
    fn a_rolling_pass_stops_after_tens_of_metres() {
        // Integrate a ball rolling at 10 m/s.
        let (mut velocity, mut distance, dt) = (Vec3::X * 10.0, 0.0, 1.0 / 60.0);
        for _ in 0..60 * 30 {
            let force = rolling(velocity, dt) + drag(velocity);
            velocity += force / MASS * dt;
            distance += velocity.x * dt;
        }
        assert!(velocity.length() < 0.01, "{velocity}");
        assert!((25.0..60.0).contains(&distance), "{distance}");
    }

    #[test]
    fn rolling_resistance_never_pushes_backwards() {
        let slow = Vec3::X * 0.01;
        let force = rolling(slow, 1.0 / 60.0);
        assert!(force.x <= 0.0 && (slow + force / MASS / 60.0).x >= -1e-6);
    }
}
