//! The controlled player: a PES footballer moved with the keyboard or a
//! gamepad, who runs with the ball at his feet, passes and shoots.
//!
//! Keyboard: arrows or ZQSD (WASD on QWERTY) to move, left Shift to
//! sprint, K to pass, L to shoot (hold to put more power). Gamepad: left
//! stick, right trigger to sprint, A / cross to pass, B / circle to shoot.

use std::sync::Arc;

use asset_bridge::model::{Animation, Model, Texture};
use bevy::prelude::*;
use bevy_bridge::{AnimationLayer, AnimationLayers, ModelSpawner, SpawnOptions};
use bevy_rapier3d::prelude::Velocity;

use crate::ball::{self, Ball};
use crate::pitch;

/// Top speeds, m/s. A professional footballer sprints at about 9 m/s.
const RUN_SPEED: f32 = 6.5;
const SPRINT_SPEED: f32 = 8.5;
/// With the ball at his feet, a player is slower.
const DRIBBLE_FACTOR: f32 = 0.85;
/// How fast the player reaches the speed asked for, m/s².
const ACCELERATION: f32 = 14.0;
/// How fast he turns, radians per second.
const TURN_RATE: f32 = 9.0;
/// The ball can be played when it is this close to the feet, horizontally.
const REACH: f32 = 1.0;
/// Ground pass speed, m/s.
const PASS_SPEED: f32 = 15.0;
/// Seconds of holding the shot button for full power.
const CHARGE_TIME: f32 = 1.0;

/// How the player moves, from the slowest: each has its animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gait {
    Idle,
    Walk,
    Jog,
    Run,
    Sprint,
}

impl Gait {
    /// The gait for a ground speed in m/s.
    pub fn for_speed(speed: f32) -> Self {
        match speed {
            s if s < 0.3 => Gait::Idle,
            s if s < 2.5 => Gait::Walk,
            s if s < 4.5 => Gait::Jog,
            s if s < 7.2 => Gait::Run,
            _ => Gait::Sprint,
        }
    }
}

/// An animation and the ground speed (m/s) its legs match at rate 1.
#[derive(Clone)]
pub struct Stride {
    pub animation: Arc<Animation>,
    pub speed: f32,
}

/// Everything needed to spawn and animate the player.
#[derive(Resource)]
pub struct PlayerAssets {
    pub model: Arc<Model>,
    pub textures: Vec<Texture>,
    /// For each [`Gait`] in order.
    pub gaits: [Stride; 5],
    pub pass: Arc<Animation>,
    pub shot: Arc<Animation>,
    /// Height of the soles below the model's origin, in PES units.
    pub soles: f32,
}

/// What the player is asked to do this frame.
#[derive(Resource, Default)]
struct Command {
    /// On the ground, x towards +X (screen right), y towards -Z (away from
    /// the camera); length up to 1.
    direction: Vec2,
    sprint: bool,
    pass: bool,
    /// Held down.
    shoot: bool,
}

/// The player under control.
#[derive(Component)]
pub struct Controlled {
    velocity: Vec3,
    /// Unit vector on the ground.
    facing: Vec3,
    gait: Gait,
    /// The kick animation playing, and the seconds left of it.
    kick: Option<f32>,
    /// Seconds the shot button has been held.
    charge: Option<f32>,
    /// Seconds before the ball can be played again after a kick.
    cooldown: f32,
    /// The entity of the spawned model, which plays the animations.
    body: Entity,
}

/// Shot power shown on screen.
#[derive(Component)]
pub struct PowerGauge;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Command>().add_systems(
            Update,
            (read_command, move_player, play_ball, animate, show_power).chain(),
        );
    }
}

/// Spawns the player at `position`, facing `facing`.
pub fn spawn(spawner: &mut ModelSpawner, assets: &PlayerAssets, position: Vec3, facing: Vec3) {
    let (body, _) = spawner.spawn(&assets.model, &assets.textures, SpawnOptions::default());
    let scale = 1.0 / retarget::PES6_UNITS_PER_METRE;
    let idle = &assets.gaits[0];
    let commands = spawner.commands();
    commands.entity(body).insert((
        Transform::from_xyz(0.0, -assets.soles * scale, 0.0).with_scale(Vec3::splat(scale)),
        AnimationLayers(vec![
            AnimationLayer::new(idle.animation.clone(), &assets.model, true).looping(),
        ]),
    ));
    commands
        .spawn((
            Transform::from_translation(position).with_rotation(facing_rotation(facing)),
            Visibility::default(),
            Controlled {
                velocity: Vec3::ZERO,
                facing,
                gait: Gait::Idle,
                kick: None,
                charge: None,
                cooldown: 0.0,
                body,
            },
        ))
        .add_child(body);
}

/// Puts the player back at `position`, still.
pub fn reset(controlled: &mut Controlled, transform: &mut Transform, position: Vec3, facing: Vec3) {
    controlled.velocity = Vec3::ZERO;
    controlled.facing = facing;
    controlled.charge = None;
    *transform = Transform::from_translation(position).with_rotation(facing_rotation(facing));
}

/// PES bodies face +Z.
fn facing_rotation(facing: Vec3) -> Quat {
    Quat::from_rotation_arc(Vec3::Z, facing.normalize_or(Vec3::Z))
}

fn read_command(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut command: ResMut<Command>,
) {
    let key = |codes: &[KeyCode]| codes.iter().any(|&code| keys.pressed(code));
    let mut direction = Vec2::new(
        f32::from(key(&[KeyCode::ArrowRight, KeyCode::KeyD]))
            - f32::from(key(&[KeyCode::ArrowLeft, KeyCode::KeyA])),
        f32::from(key(&[KeyCode::ArrowUp, KeyCode::KeyW]))
            - f32::from(key(&[KeyCode::ArrowDown, KeyCode::KeyS])),
    );
    let mut sprint = keys.pressed(KeyCode::ShiftLeft);
    let mut pass = keys.just_pressed(KeyCode::KeyK);
    let mut shoot = keys.pressed(KeyCode::KeyL);
    for gamepad in &gamepads {
        let stick = gamepad.left_stick();
        if stick.length() > 0.15 {
            direction = stick;
        }
        sprint |= gamepad.pressed(GamepadButton::RightTrigger2);
        pass |= gamepad.just_pressed(GamepadButton::South);
        shoot |= gamepad.pressed(GamepadButton::East);
    }
    *command = Command {
        direction: direction.clamp_length_max(1.0),
        sprint,
        pass,
        shoot,
    };
}

/// The velocity the command asks for, on the ground.
pub fn wanted_velocity(direction: Vec2, sprint: bool, dribbling: bool) -> Vec3 {
    let top = if sprint { SPRINT_SPEED } else { RUN_SPEED };
    let top = if dribbling { top * DRIBBLE_FACTOR } else { top };
    Vec3::new(direction.x, 0.0, -direction.y) * top
}

fn move_player(
    time: Res<Time>,
    command: Res<Command>,
    balls: Query<&Transform, (With<Ball>, Without<Controlled>)>,
    mut players: Query<(&mut Controlled, &mut Transform)>,
) {
    let dt = time.delta_secs();
    let ball = balls.single().ok().map(|t| t.translation);
    for (mut player, mut transform) in &mut players {
        let dribbling = ball.is_some_and(|ball| within_reach(transform.translation, ball))
            && player.kick.is_none();
        // A kick plants the feet: no running during the kick.
        let wanted = if player.kick.is_some() {
            Vec3::ZERO
        } else {
            wanted_velocity(command.direction, command.sprint, dribbling)
        };
        let change = (wanted - player.velocity).clamp_length_max(ACCELERATION * dt);
        player.velocity += change;
        if wanted.length() > 0.1 {
            let target = wanted.normalize();
            let angle = player.facing.angle_between(target);
            let step = (TURN_RATE * dt / angle.max(1e-4)).min(1.0);
            player.facing = player.facing.slerp(target, step).normalize_or(target);
        }
        let next = transform.translation + player.velocity * dt;
        // The boards stop the player.
        let (hl, hw) = (pitch::BOARD_X - 0.5, pitch::BOARD_Z - 0.5);
        transform.translation = Vec3::new(next.x.clamp(-hl, hl), 0.0, next.z.clamp(-hw, hw));
        transform.rotation = facing_rotation(player.facing);
        player.cooldown = (player.cooldown - dt).max(0.0);
    }
}

/// Whether the ball at `ball` is at the feet of a player at `player`.
pub fn within_reach(player: Vec3, ball: Vec3) -> bool {
    let offset = ball - player;
    Vec2::new(offset.x, offset.z).length() < REACH && ball.y < 0.6
}

/// Where the ball sits while dribbling: just in front of the feet.
fn dribble_spot(player: Vec3, facing: Vec3, speed: f32) -> Vec3 {
    // Further ahead at speed, as players push the ball forwards.
    player + facing * (0.45 + 0.08 * speed) + Vec3::Y * ball::RADIUS
}

/// The ball's velocity while dribbling: the player's, plus a pull towards
/// the spot in front of his feet.
pub fn dribble_velocity(player: Vec3, velocity: Vec3, facing: Vec3, ball: Vec3) -> Vec3 {
    let spot = dribble_spot(player, facing, velocity.length());
    let pull = (spot - ball) * 8.0;
    Vec3::new(velocity.x + pull.x, 0.0, velocity.z + pull.z)
}

/// A ground pass along `facing`.
pub fn pass_velocity(facing: Vec3) -> Vec3 {
    facing * PASS_SPEED + Vec3::Y * 0.6
}

/// A shot along `facing` with `power` from 0 to 1: faster and higher with
/// power.
pub fn shot_velocity(facing: Vec3, power: f32) -> Vec3 {
    let power = power.clamp(0.0, 1.0);
    let speed = 16.0 + 16.0 * power;
    let elevation = (4.0 + 12.0 * power).to_radians();
    (facing * elevation.cos() + Vec3::Y * elevation.sin()) * speed
}

fn play_ball(
    time: Res<Time>,
    command: Res<Command>,
    assets: Res<PlayerAssets>,
    mut players: Query<(&mut Controlled, &Transform)>,
    mut balls: Query<(&Transform, &mut Velocity), With<Ball>>,
    mut layers: Query<&mut AnimationLayers>,
) {
    let dt = time.delta_secs();
    let Ok((ball_transform, mut ball_velocity)) = balls.single_mut() else {
        return;
    };
    let ball = ball_transform.translation;
    for (mut player, transform) in &mut players {
        if let Some(left) = player.kick.as_mut() {
            *left -= dt;
            if *left <= 0.0 {
                player.kick = None;
            }
        }
        let reach = player.cooldown <= 0.0 && within_reach(transform.translation, ball);
        // Shot: charge while held, kick on release.
        let idle = player.kick.is_none();
        let released = match (player.charge, command.shoot) {
            (Some(charge), true) => {
                player.charge = Some(charge + dt);
                None
            }
            (Some(charge), false) => Some(charge / CHARGE_TIME),
            (None, true) if idle => {
                player.charge = Some(0.0);
                None
            }
            (None, _) => None,
        };
        let kick = if let Some(power) = released {
            player.charge = None;
            reach.then(|| (shot_velocity(player.facing, power), assets.shot.clone()))
        } else if command.pass && reach && player.kick.is_none() {
            Some((pass_velocity(player.facing), assets.pass.clone()))
        } else {
            None
        };
        if let Some((velocity, animation)) = kick {
            ball_velocity.linear = velocity;
            // A little backspin on the ground pass, none on the shot.
            ball_velocity.angular = Vec3::ZERO;
            player.cooldown = 0.35;
            player.kick = Some(animation.duration.min(0.7));
            if let Ok(mut layers) = layers.get_mut(player.body) {
                layers.0 = vec![AnimationLayer::new(animation, &assets.model, true)];
            }
            continue;
        }
        if reach && player.kick.is_none() && player.charge.is_none() {
            let velocity =
                dribble_velocity(transform.translation, player.velocity, player.facing, ball);
            ball_velocity.linear = Vec3::new(velocity.x, ball_velocity.linear.y, velocity.z);
            // Rolling, not sliding.
            ball_velocity.angular = Vec3::Y.cross(velocity) / ball::RADIUS;
        }
    }
}

/// Plays the animation of the gait matching the speed, its pace following
/// the actual speed; kicks play to their end first.
fn animate(
    assets: Res<PlayerAssets>,
    mut players: Query<&mut Controlled>,
    mut layers: Query<&mut AnimationLayers>,
) {
    for mut player in &mut players {
        if player.kick.is_some() {
            continue;
        }
        let Ok(mut layers) = layers.get_mut(player.body) else {
            continue;
        };
        let speed = Vec2::new(player.velocity.x, player.velocity.z).length();
        let gait = Gait::for_speed(speed);
        let stride = &assets.gaits[gait as usize];
        if gait != player.gait || layers.0.is_empty() {
            // Keep the phase of the stride when changing gait.
            let phase = layers
                .0
                .first()
                .map(|layer| layer.time / layer.animation().duration.max(1e-3))
                .unwrap_or(0.0)
                .fract();
            let mut layer =
                AnimationLayer::new(stride.animation.clone(), &assets.model, true).looping();
            layer.time = phase * stride.animation.duration;
            layers.0 = vec![layer];
            player.gait = gait;
        }
        if let Some(layer) = layers.0.first_mut() {
            layer.rate = if gait == Gait::Idle {
                1.0
            } else {
                (speed / stride.speed.max(0.1)).clamp(0.5, 1.6)
            };
        }
    }
}

fn show_power(players: Query<&Controlled>, mut gauges: Query<&mut Text, With<PowerGauge>>) {
    let power = players
        .iter()
        .find_map(|player| player.charge)
        .map(|charge| (charge / CHARGE_TIME).min(1.0));
    for mut text in &mut gauges {
        text.0 = match power {
            Some(power) => {
                let filled = (power * 20.0).round() as usize;
                format!("Tir {}{}", "█".repeat(filled), "░".repeat(20 - filled))
            }
            None => String::new(),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaits_follow_the_speed() {
        assert_eq!(Gait::for_speed(0.0), Gait::Idle);
        assert_eq!(Gait::for_speed(1.5), Gait::Walk);
        assert_eq!(Gait::for_speed(RUN_SPEED), Gait::Run);
        assert_eq!(Gait::for_speed(SPRINT_SPEED), Gait::Sprint);
    }

    #[test]
    fn up_on_the_stick_runs_away_from_the_camera() {
        let velocity = wanted_velocity(Vec2::Y, false, false);
        assert!(velocity.z < 0.0 && velocity.x == 0.0);
        assert_eq!(velocity.length(), RUN_SPEED);
        assert!(wanted_velocity(Vec2::X, true, true).length() < SPRINT_SPEED);
    }

    #[test]
    fn the_ball_is_pulled_in_front_of_the_feet() {
        let player = Vec3::ZERO;
        // Behind the player: pulled forwards.
        let behind = dribble_velocity(player, Vec3::ZERO, Vec3::X, Vec3::new(-0.3, 0.11, 0.0));
        assert!(behind.x > 0.0 && behind.y == 0.0);
        // On the spot, the ball moves with the player.
        let spot = dribble_spot(player, Vec3::X, 5.0);
        let with = dribble_velocity(player, Vec3::X * 5.0, Vec3::X, spot);
        assert!((with - Vec3::X * 5.0).length() < 1e-4);
    }

    #[test]
    fn shots_go_faster_and_higher_with_power() {
        let (soft, hard) = (shot_velocity(Vec3::X, 0.0), shot_velocity(Vec3::X, 1.0));
        assert!(hard.length() > soft.length() && hard.y > soft.y);
        assert!(shot_velocity(Vec3::X, 5.0) == hard);
        // A pass stays on the ground.
        assert!(pass_velocity(Vec3::X).y < 1.0);
    }

    #[test]
    fn reach_is_around_the_feet_and_low() {
        assert!(within_reach(Vec3::ZERO, Vec3::new(0.5, 0.11, 0.3)));
        assert!(!within_reach(Vec3::ZERO, Vec3::new(1.5, 0.11, 0.0)));
        assert!(!within_reach(Vec3::ZERO, Vec3::new(0.3, 1.5, 0.0)));
    }
}
