//! Chaos FC: the Bevy application.
//!
//! Milestone J3: in an empty scene, Tommy holds a Colt 45 and shoots with the
//! left mouse button, with the animation, timing and sound of Vice City.

mod shooting;

use std::sync::Arc;

use asset_bridge::config::{self, GamePaths};
use asset_bridge::model::{Animation, Model, Texture, Weapon};
use asset_bridge::vice_city::{self, ViceCity, ViceCityError};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy_bridge::{
    AnimationLayer, AnimationLayers, BevyBridgePlugin, BindPose, ModelSpawner, SpawnOptions,
};

use crate::shooting::{Shooter, ShootingPlugin, Target};

/// What the scene needs from the player's Vice City, loaded before start.
#[derive(Resource)]
struct SceneAssets {
    tommy: Arc<Model>,
    tommy_textures: Vec<Texture>,
    /// Body animation while standing.
    idle: Arc<Animation>,
    colt: Arc<Weapon>,
}

fn main() -> AppExit {
    // The game never starts without valid paths to the player's own copies
    // of the original games.
    let paths = match config::load_game_paths(&config::config_path()) {
        Ok(paths) => paths,
        Err(err) => {
            eprintln!("Chaos FC ne peut pas démarrer.\n\n{err}");
            return AppExit::error();
        }
    };
    let assets = match load_assets(&paths) {
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
                title: "Chaos FC".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((BevyBridgePlugin, ShootingPlugin))
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
        colt: Arc::new(game.load_weapon(&vice_city::COLT45)?),
    })
}

/// Ground, targets, light, camera and help text.
fn setup_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
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
            "Chaos FC - jalon J3\n\
             Clic gauche (maintenu) : tirer avec le Colt 45\n\
             Clic droit + glisser : tourner la caméra, molette : zoom",
        ),
        TextFont::from_font_size(15.0),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
}

/// Tommy, standing in place, the Colt 45 in his right hand.
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
    let (colt, _) = spawner.spawn(
        &assets.colt.model,
        &assets.colt.textures,
        SpawnOptions::default(),
    );
    let hand = body.node("R Hand").expect("player.dff has an R Hand bone");

    let commands = spawner.commands();
    commands.entity(tommy).insert((
        // The soles are a few centimetres below the lowest bones.
        Transform::from_rotation(rotation).with_translation(Vec3::Y * (0.03 - feet)),
        AnimationLayers(vec![
            AnimationLayer::new(assets.idle.clone(), &assets.tommy, true).looping(),
        ]),
        Shooter::new(
            assets.colt.clone(),
            assets.tommy.clone(),
            colt,
            audio_sources.add(bevy_bridge::audio_source(&assets.colt.fire_sound)),
        ),
    ));
    // HYPOTHÈSE: Vice City attaches the weapon model to the hand bone as is;
    // to be confirmed by eye (the grip must sit in the hand).
    commands.entity(colt).insert(ChildOf(hand));
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
