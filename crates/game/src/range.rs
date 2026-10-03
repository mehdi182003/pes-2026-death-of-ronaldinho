//! The shooting range of milestone J3: in an empty scene, Tommy holds a
//! weapon of Vice City (Colt 45, Uzi or Ruger, keys 1 to 3), aims with the
//! mouse and shoots with the left button, with the animation, timing and
//! sound of the original game.

use std::sync::Arc;

use asset_bridge::config::GamePaths;
use asset_bridge::model::{Animation, Model, Texture, Weapon};
use asset_bridge::vice_city::{self, ViceCity, ViceCityError};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy_bridge::{
    AnimationLayer, AnimationLayers, BevyBridgePlugin, BindPose, ModelSpawner, SpawnOptions,
};

use crate::capture::CapturePlugin;
use crate::shooting::{Carried, Shooter, ShootingPlugin, Target, WeaponLabel};

/// What the scene needs from the player's Vice City, loaded before start.
#[derive(Resource)]
struct SceneAssets {
    tommy: Arc<Model>,
    tommy_textures: Vec<Texture>,
    /// Body animation while standing.
    idle: Arc<Animation>,
    /// In the order of the keys 1, 2, 3.
    weapons: Vec<Arc<Weapon>>,
}

/// Runs the shooting range.
pub fn run(paths: &GamePaths, capture: CapturePlugin) -> AppExit {
    let assets = match load_assets(paths) {
        Ok(assets) => assets,
        Err(err) => {
            eprintln!(
                "Chaos FC ne peut pas démarrer : lecture de GTA Vice City impossible.\n\n{err}"
            );
            return AppExit::error();
        }
    };

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Chaos FC - stand de tir".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            BevyBridgePlugin,
            crate::HudFontPlugin,
            ShootingPlugin,
            capture,
        ))
        .insert_resource(ClearColor(Color::srgb(0.55, 0.72, 0.88)))
        .insert_resource(GlobalAmbientLight {
            brightness: 500.0,
            ..default()
        })
        .insert_resource(assets)
        .add_systems(Startup, (setup_world, spawn_tommy))
        .add_systems(Update, orbit_camera)
        .run()
}

fn load_assets(paths: &GamePaths) -> Result<SceneAssets, ViceCityError> {
    let mut game = ViceCity::open(&paths.vice_city)?;
    let idle = game
        .load_animations("ped")?
        .into_iter()
        .find(|animation| animation.name.eq_ignore_ascii_case("IDLE_stance"))
        .ok_or_else(|| ViceCityError::Missing("IDLE_stance absente de ped.ifp".into()))?;
    Ok(SceneAssets {
        tommy: Arc::new(game.load_model("player")?),
        tommy_textures: game.load_textures("player")?,
        idle: Arc::new(idle),
        weapons: [vice_city::COLT45, vice_city::UZI, vice_city::RUGER]
            .iter()
            .map(|spec| game.load_weapon(spec).map(Arc::new))
            .collect::<Result<_, _>>()?,
    })
}

/// Ground, targets, light, camera and help text.
fn setup_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    font: Res<crate::HudFont>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(60.0, 60.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.35, 0.45, 0.30))),
    ));

    // Tommy faces -Z: targets at increasing distances in front of him.
    let target_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    for (position, color) in [
        (Vec3::new(0.0, 1.0, -6.0), Color::srgb(0.8, 0.3, 0.2)),
        (Vec3::new(-2.5, 0.5, -12.0), Color::srgb(0.9, 0.8, 0.2)),
        (Vec3::new(3.0, 1.5, -20.0), Color::srgb(0.3, 0.5, 0.9)),
    ] {
        commands.spawn((
            Mesh3d(target_mesh.clone()),
            MeshMaterial3d(materials.add(color)),
            Transform::from_translation(position),
            Target {
                half_size: Vec3::splat(0.5),
            },
        ));
    }

    commands.spawn((
        DirectionalLight {
            illuminance: 8000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    let orbit = Orbit {
        target: Vec3::new(0.0, 1.2, 0.0),
        yaw: 0.35,
        pitch: -0.2,
        distance: 3.5,
    };
    commands.spawn((Camera3d::default(), orbit.transform(), orbit));
    commands.spawn((
        Text::new(
            "Chaos FC - stand de tir (jalon J3)\n\
             Souris : viser, clic gauche (maintenu) : tirer\n\
             Touches 1, 2, 3 : Colt 45, Uzi, Ruger\n\
             Clic droit + glisser : tourner la caméra, molette : zoom",
        ),
        font.text(15.0),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
    commands.spawn((
        WeaponLabel,
        Text::new(""),
        font.text(22.0),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(16.0),
            right: Val::Px(20.0),
            ..default()
        },
    ));
}

/// Tommy, standing in place, his weapons in his right hand (only the one he
/// holds is visible).
fn spawn_tommy(
    mut spawner: ModelSpawner,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    assets: Res<SceneAssets>,
) {
    let rotation = Quat::from_array(retarget::VICE_CITY_TO_Y_UP);
    let feet = lowest_point(&assets.tommy, &assets.idle, rotation);
    let (tommy, body) = spawner.spawn(
        &assets.tommy,
        &assets.tommy_textures,
        SpawnOptions::default(),
    );
    let hand = body.node("R Hand").expect("player.dff has an R Hand bone");
    let arsenal: Vec<Carried> = assets
        .weapons
        .iter()
        .enumerate()
        .map(|(index, weapon)| {
            let (entity, _) =
                spawner.spawn(&weapon.model, &weapon.textures, SpawnOptions::default());
            // The weapon model goes on the hand bone as is: confirmed by
            // eye, the grip sits in the hand.
            let visibility = if index == 0 {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            spawner
                .commands()
                .entity(entity)
                .insert((ChildOf(hand), visibility));
            Carried {
                weapon: weapon.clone(),
                fire_animation: Arc::new(weapon.fire_animation.clone()),
                entity,
                sound: audio_sources.add(bevy_bridge::audio_source(&weapon.fire_sound)),
            }
        })
        .collect();

    let commands = spawner.commands();
    commands.entity(tommy).insert((
        // The soles are a few centimetres below the lowest bones.
        Transform::from_rotation(rotation).with_translation(Vec3::Y * (0.03 - feet)),
        AnimationLayers(vec![
            AnimationLayer::new(assets.idle.clone(), &assets.tommy, true).looping(),
        ]),
        Shooter::new(arsenal, rotation, assets.tommy.clone()),
    ));
}

/// Height of the lowest node of `model` at the start of `animation`, once
/// turned by `rotation`.
fn lowest_point(model: &Model, animation: &Arc<Animation>, rotation: Quat) -> f32 {
    let mut locals: Vec<Transform> = BindPose::of(model)
        .locals
        .iter()
        .map(|&local| Transform::from_matrix(local))
        .collect();
    AnimationLayer::new(animation.clone(), model, true).apply_at(0.0, &mut locals);
    let root = Mat4::from_quat(rotation);
    let mut world: Vec<Mat4> = Vec::with_capacity(locals.len());
    let mut lowest = f32::MAX;
    for (node, local) in model.nodes.iter().zip(&locals) {
        let matrix = node.parent.map_or(root, |p| world[p]) * local.to_matrix();
        lowest = lowest.min(matrix.w_axis.y);
        world.push(matrix);
    }
    lowest
}

#[derive(Component)]
struct Orbit {
    target: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
}

impl Orbit {
    fn transform(&self) -> Transform {
        let rotation = Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0);
        Transform::from_translation(self.target + rotation * Vec3::new(0.0, 0.0, self.distance))
            .looking_at(self.target, Vec3::Y)
    }
}

/// Right button: turn around Tommy (the left one shoots). Wheel: zoom.
fn orbit_camera(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut cameras: Query<(&mut Orbit, &mut Transform)>,
) {
    let Ok((mut orbit, mut transform)) = cameras.single_mut() else {
        return;
    };
    if buttons.pressed(MouseButton::Right) {
        orbit.yaw -= motion.delta.x * 0.008;
        orbit.pitch = (orbit.pitch - motion.delta.y * 0.008).clamp(-1.4, 1.2);
    }
    let notches = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 100.0,
    };
    if notches != 0.0 {
        orbit.distance = (orbit.distance * 0.9f32.powf(notches)).clamp(0.8, 30.0);
    }
    *transform = orbit.transform();
}
