//! Shooting: the mouse cursor aims, the shooter turns towards the aimed
//! point, the firing animation of the weapon plays over the body, and a
//! round leaves the barrel towards the aimed point at each firing point of
//! the loop, with its sound and visible effects.

use std::sync::Arc;

use asset_bridge::model::{Animation, Model, Weapon};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_bridge::{AnimationLayer, AnimationLayers};

/// How far the cursor can aim when it points at nothing (sky).
const AIM_DISTANCE: f32 = 50.0;

pub struct ShootingPlugin;

impl Plugin for ShootingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Aim>().add_systems(
            Update,
            (
                switch_weapon,
                aim_with_cursor,
                face_aim,
                pull_trigger,
                fire_rounds,
                expire_effects,
                draw_effects,
                show_weapon,
            )
                .chain(),
        );
    }
}

/// Point under the mouse cursor: on a target or the ground, or far away
/// along the cursor ray.
#[derive(Resource, Default)]
struct Aim(Option<Vec3>);

/// A weapon carried by a shooter.
pub struct Carried {
    pub weapon: Arc<Weapon>,
    pub fire_animation: Arc<Animation>,
    /// Root entity of the weapon model, in the hand (hidden when another
    /// weapon is held).
    pub entity: Entity,
    pub sound: Handle<AudioSource>,
}

/// A character holding a firearm, chosen among those it carries.
#[derive(Component)]
pub struct Shooter {
    arsenal: Vec<Carried>,
    /// Index of the weapon in hand.
    current: usize,
    /// Rotation of the character when facing -Z (its model turned upright).
    upright: Quat,
    /// The character's model, to bind the firing animation to its bones.
    body: Arc<Model>,
    /// Time of the firing layer at the previous frame, to detect when it
    /// passes the firing point.
    last_time: Option<f32>,
}

impl Shooter {
    /// Holds the first weapon of `arsenal`.
    pub fn new(arsenal: Vec<Carried>, upright: Quat, body: Arc<Model>) -> Self {
        Self {
            arsenal,
            current: 0,
            upright,
            body,
            last_time: None,
        }
    }

    fn held(&self) -> &Carried {
        &self.arsenal[self.current]
    }

    /// Name of the weapon in hand.
    pub fn held_name(&self) -> &str {
        &self.held().weapon.name
    }

    fn is_held_fire_layer(&self, layer: &AnimationLayer) -> bool {
        layer.animation().name == self.held().fire_animation.name
    }
}

/// Keys 1 to 9 take the corresponding weapon (same place on AZERTY and
/// QWERTY keyboards: the physical key counts, not its character).
fn switch_weapon(
    keys: Res<ButtonInput<KeyCode>>,
    mut shooters: Query<(&mut Shooter, &mut AnimationLayers)>,
    mut visibilities: Query<&mut Visibility>,
) {
    const DIGITS: [KeyCode; 9] = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    let Some(wanted) = DIGITS.iter().position(|&key| keys.just_pressed(key)) else {
        return;
    };
    for (mut shooter, mut layers) in &mut shooters {
        if wanted >= shooter.arsenal.len() || wanted == shooter.current {
            continue;
        }
        // Stop the firing animation of the previous weapon.
        let fire_names: Vec<&str> = shooter
            .arsenal
            .iter()
            .map(|carried| carried.fire_animation.name.as_str())
            .collect();
        layers
            .0
            .retain(|layer| !fire_names.contains(&layer.animation().name.as_str()));
        shooter.current = wanted;
        shooter.last_time = None;
        for (index, carried) in shooter.arsenal.iter().enumerate() {
            if let Ok(mut visibility) = visibilities.get_mut(carried.entity) {
                *visibility = if index == wanted {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
    }
}

/// Something that stops rounds: an axis-aligned box around its translation.
#[derive(Component)]
pub struct Target {
    pub half_size: Vec3,
}

/// Casts the cursor ray from the camera into the scene.
fn aim_with_cursor(
    mut aim: ResMut<Aim>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    targets: Query<(&GlobalTransform, &Target)>,
) {
    let ray = windows
        .single()
        .ok()
        .and_then(Window::cursor_position)
        .zip(cameras.single().ok())
        .and_then(|(cursor, (camera, transform))| camera.viewport_to_world(transform, cursor).ok());
    aim.0 = ray.map(|ray| {
        let direction = *ray.direction;
        let distance = cast_ray(ray.origin, direction, AIM_DISTANCE, &targets);
        ray.origin + direction * distance.unwrap_or(AIM_DISTANCE)
    });
}

/// The shooter turns on the spot to face the aimed point.
fn face_aim(aim: Res<Aim>, mut shooters: Query<(&Shooter, &mut Transform)>) {
    let Some(point) = aim.0 else {
        return;
    };
    for (shooter, mut transform) in &mut shooters {
        let flat = (point - transform.translation) * Vec3::new(1.0, 0.0, 1.0);
        if flat.length_squared() < 0.01 {
            continue;
        }
        // Yaw that turns -Z (the forward of the upright model) towards it.
        let yaw = (-flat.x).atan2(-flat.z);
        transform.rotation = Quat::from_rotation_y(yaw) * shooter.upright;
    }
}

/// Holding the left button raises the arm and loops the firing part of the
/// animation; releasing it lets the animation finish (the arm comes down).
fn pull_trigger(
    buttons: Res<ButtonInput<MouseButton>>,
    mut shooters: Query<(&Shooter, &mut AnimationLayers)>,
) {
    for (shooter, mut layers) in &mut shooters {
        let [loop_start, loop_end, _] = shooter.held().weapon.fire_loop;
        let holding = buttons.pressed(MouseButton::Left);
        match layers
            .0
            .iter_mut()
            .find(|layer| shooter.is_held_fire_layer(layer))
        {
            Some(layer) => {
                layer.loop_range = holding.then_some((loop_start, loop_end));
            }
            None if holding => {
                let mut layer = AnimationLayer::new(
                    shooter.held().fire_animation.clone(),
                    &shooter.body,
                    false,
                );
                layer.loop_range = Some((loop_start, loop_end));
                layers.0.push(layer);
            }
            None => {}
        }
    }
}

/// A round leaves the barrel each time the firing animation passes its
/// firing point.
fn fire_rounds(
    aim: Res<Aim>,
    mut commands: Commands,
    mut shooters: Query<(&mut Shooter, &AnimationLayers)>,
    globals: Query<&GlobalTransform>,
    targets: Query<(&GlobalTransform, &Target)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (mut shooter, layers) in &mut shooters {
        let Some(time) = layers
            .0
            .iter()
            .find(|layer| shooter.is_held_fire_layer(layer))
            .map(|layer| layer.time)
        else {
            shooter.last_time = None;
            continue;
        };
        let fire_point = shooter.held().weapon.fire_loop[2];
        let crossed = match shooter.last_time {
            None => time >= fire_point && time < fire_point + 0.1,
            Some(last) if time >= last => last < fire_point && fire_point <= time,
            // The loop went back to its start.
            Some(last) => fire_point > last || fire_point <= time,
        };
        shooter.last_time = Some(time);
        if !crossed {
            continue;
        }
        let carried = shooter.held();
        let Ok(weapon) = globals.get(carried.entity) else {
            continue;
        };

        // The muzzle offset of weapon.dat is in the weapon's space, the
        // barrel along its +X: confirmed by eye (flash and tracer leave the
        // barrel). As in GTA, the round flies towards the aimed point, not
        // along the barrel of the animated arm.
        let muzzle = weapon.transform_point(Vec3::from_array(carried.weapon.muzzle));
        let direction = aim
            .0
            .map(|point| point - muzzle)
            .filter(|towards| towards.length_squared() > 1e-4)
            .unwrap_or(weapon.rotation() * Vec3::X)
            .normalize();
        let range = carried.weapon.range;
        let hit = cast_ray(muzzle, direction, range, &targets);
        let end = muzzle + direction * hit.unwrap_or(range);

        commands.spawn((
            AudioPlayer::new(carried.sound.clone()),
            PlaybackSettings::DESPAWN,
        ));
        commands.spawn((
            Tracer {
                from: muzzle,
                to: end,
            },
            Lifetime(Timer::from_seconds(0.08, TimerMode::Once)),
        ));
        commands.spawn((
            Mesh3d(meshes.add(Sphere::new(0.05))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.85, 0.4),
                emissive: LinearRgba::rgb(8.0, 5.0, 1.5),
                unlit: true,
                ..default()
            })),
            Transform::from_translation(muzzle),
            Lifetime(Timer::from_seconds(0.05, TimerMode::Once)),
        ));
        if hit.is_some() {
            commands.spawn((
                Mesh3d(meshes.add(Sphere::new(0.04))),
                MeshMaterial3d(materials.add(Color::srgb(0.1, 0.1, 0.1))),
                Transform::from_translation(end),
                Lifetime(Timer::from_seconds(4.0, TimerMode::Once)),
            ));
        }
    }
}

/// Distance to the closest hit along the ray, among the targets and the
/// ground (y = 0), within `range`.
fn cast_ray(
    origin: Vec3,
    direction: Vec3,
    range: f32,
    targets: &Query<(&GlobalTransform, &Target)>,
) -> Option<f32> {
    let ground = (direction.y < 0.0).then(|| -origin.y / direction.y);
    targets
        .iter()
        .filter_map(|(transform, target)| {
            ray_box(
                origin,
                direction,
                transform.translation() - target.half_size,
                transform.translation() + target.half_size,
            )
        })
        .chain(ground)
        .filter(|&distance| distance >= 0.0 && distance <= range)
        .min_by(f32::total_cmp)
}

/// Entry distance of a ray into an axis-aligned box (slab method).
fn ray_box(origin: Vec3, direction: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let inverse = direction.recip();
    let near = (min - origin) * inverse;
    let far = (max - origin) * inverse;
    let enter = near.min(far).max_element();
    let exit = near.max(far).min_element();
    (enter <= exit && exit >= 0.0).then_some(enter.max(0.0))
}

/// The line of a round, visible for a moment.
#[derive(Component)]
struct Tracer {
    from: Vec3,
    to: Vec3,
}

/// Despawned when the timer ends.
#[derive(Component)]
struct Lifetime(Timer);

fn expire_effects(
    mut commands: Commands,
    time: Res<Time>,
    mut effects: Query<(Entity, &mut Lifetime)>,
) {
    for (entity, mut lifetime) in &mut effects {
        if lifetime.0.tick(time.delta()).is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

/// Tracers, and the crosshair on the aimed point.
fn draw_effects(mut gizmos: Gizmos, aim: Res<Aim>, tracers: Query<&Tracer>) {
    for tracer in &tracers {
        gizmos.line(tracer.from, tracer.to, Color::srgb(1.0, 0.9, 0.5));
    }
    if let Some(point) = aim.0 {
        gizmos.sphere(
            Isometry3d::from_translation(point),
            0.08,
            Color::srgb(1.0, 0.2, 0.2),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rays_enter_boxes_in_front_only() {
        let (min, max) = (Vec3::new(-0.5, 0.0, -6.5), Vec3::new(0.5, 1.0, -5.5));
        let hit = ray_box(Vec3::new(0.0, 0.5, 0.0), Vec3::NEG_Z, min, max).unwrap();
        assert!((hit - 5.5).abs() < 1e-5, "{hit}");
        assert_eq!(ray_box(Vec3::new(0.0, 0.5, 0.0), Vec3::Z, min, max), None);
        assert_eq!(
            ray_box(Vec3::new(3.0, 0.5, 0.0), Vec3::NEG_Z, min, max),
            None
        );
    }
}

/// Text showing the weapon in hand.
#[derive(Component)]
pub struct WeaponLabel;

fn show_weapon(shooters: Query<&Shooter>, mut labels: Query<&mut Text, With<WeaponLabel>>) {
    let Ok(shooter) = shooters.single() else {
        return;
    };
    for mut label in &mut labels {
        if label.0 != shooter.held_name() {
            label.0 = shooter.held_name().to_owned();
        }
    }
}
