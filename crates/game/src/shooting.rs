//! Shooting: the firing animation of the weapon over the body, a round at
//! each firing point of the loop, with its sound, a ray along the barrel and
//! visible effects.

use std::sync::Arc;

use asset_bridge::model::{Model, Weapon};
use bevy::prelude::*;
use bevy_bridge::{AnimationLayer, AnimationLayers};

pub struct ShootingPlugin;

impl Plugin for ShootingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (pull_trigger, fire_rounds, expire_effects, draw_tracers).chain(),
        );
    }
}

/// A character holding a firearm.
#[derive(Component)]
pub struct Shooter {
    weapon: Arc<Weapon>,
    /// The character's model, to bind the firing animation to its bones.
    body: Arc<Model>,
    /// Root entity of the weapon model, in the hand.
    weapon_entity: Entity,
    sound: Handle<AudioSource>,
    /// Time of the firing layer at the previous frame, to detect when it
    /// passes the firing point.
    last_time: Option<f32>,
}

impl Shooter {
    pub fn new(
        weapon: Arc<Weapon>,
        body: Arc<Model>,
        weapon_entity: Entity,
        sound: Handle<AudioSource>,
    ) -> Self {
        Self {
            weapon,
            body,
            weapon_entity,
            sound,
            last_time: None,
        }
    }

    fn is_fire_layer(&self, layer: &AnimationLayer) -> bool {
        layer.animation().name == self.weapon.fire_animation.name
    }
}

/// Something that stops rounds: an axis-aligned box around its translation.
#[derive(Component)]
pub struct Target {
    pub half_size: Vec3,
}

/// Holding the left button raises the arm and loops the firing part of the
/// animation; releasing it lets the animation finish (the arm comes down).
fn pull_trigger(
    buttons: Res<ButtonInput<MouseButton>>,
    mut shooters: Query<(&Shooter, &mut AnimationLayers)>,
) {
    for (shooter, mut layers) in &mut shooters {
        let [loop_start, loop_end, _] = shooter.weapon.fire_loop;
        let holding = buttons.pressed(MouseButton::Left);
        match layers
            .0
            .iter_mut()
            .find(|layer| shooter.is_fire_layer(layer))
        {
            Some(layer) => {
                layer.loop_range = holding.then_some((loop_start, loop_end));
            }
            None if holding => {
                let mut layer = AnimationLayer::new(
                    Arc::new(shooter.weapon.fire_animation.clone()),
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
            .find(|layer| shooter.is_fire_layer(layer))
            .map(|layer| layer.time)
        else {
            shooter.last_time = None;
            continue;
        };
        let fire_point = shooter.weapon.fire_loop[2];
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
        let Ok(weapon) = globals.get(shooter.weapon_entity) else {
            continue;
        };

        // HYPOTHÈSE: the muzzle offset of weapon.dat is in the weapon's
        // space and the barrel points along its +X (the offset of the Colt
        // 45 is mostly along X). To be confirmed by eye with the tracer.
        let muzzle = weapon.transform_point(Vec3::from_array(shooter.weapon.muzzle));
        let direction = (weapon.rotation() * Vec3::X).normalize();
        let range = shooter.weapon.range;
        let hit = cast_ray(muzzle, direction, range, &targets);
        let end = muzzle + direction * hit.unwrap_or(range);

        commands.spawn((
            AudioPlayer::new(shooter.sound.clone()),
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

fn draw_tracers(mut gizmos: Gizmos, tracers: Query<&Tracer>) {
    for tracer in &tracers {
        gizmos.line(tracer.from, tracer.to, Color::srgb(1.0, 0.9, 0.5));
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
