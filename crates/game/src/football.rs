//! The football match, in a stadium of PES 6, seen from the stand like the
//! wide camera of PES.

use std::sync::Arc;

use asset_bridge::config::GamePaths;
use asset_bridge::model::{Animation, Model, Texture};
use asset_bridge::pes6::{Pes6, Pes6Error, PesFile, PlayerParts};
use asset_bridge::vice_city::ViceCity;
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy_bridge::{BevyBridgePlugin, ModelSpawner, SpawnOptions};
use bevy_rapier3d::prelude::*;

use crate::ball::{self, Ball};
use crate::capture::CapturePlugin;
use crate::pitch;
use crate::player::{self, PlayerAssets, PlayerPlugin, PowerGauge, Stride};

/// The stadium played in: the Sapporo dome of PES 6.
const STADIUM: &str = "0_text:6949";
/// The texture of its goal nets.
const NET_TEXTURE: &str = "0_text:6949/1/43";
/// The balls of `0_text.afs`: a model, then its texture, from file 0 to
/// file 47. The match uses the first one whose texture can be read (some
/// are stored swizzled, which is not handled yet).
const BALLS: std::ops::Range<usize> = 0..24;

/// What the match needs from the player's games, loaded before start.
#[derive(Resource)]
struct MatchAssets {
    stadium: Model,
    stadium_textures: Vec<Texture>,
    ball: Model,
    ball_texture: Texture,
}

/// The controlled player: the field player of milestone J6, in the readable
/// training kit.
const PLAYER_BODY: &str = "0_text:1010";
const PLAYER_KIT: &str = "0_text:419";
const PLAYER_BOOTS: &str = "0_text:5322/0/0";
const PLAYER_HEAD: &str = "0_text:1943";
const PLAYER_HAIR: &str = "0_text:4570";

/// Vice City animations played by the PES player, retargeted (see
/// `retarget`): standing, walking, jogging, running, sprinting, in the order
/// of `player::Gait`, then the pass and the shot.
// HYPOTHÈSE: until PES animations are read, these are the closest of
// ped.ifp; KICK_floor and FIGHTlngkck are kicks of street fights.
const GAITS: [&str; 5] = [
    "IDLE_stance",
    "WALK_player",
    "JOG_maleA",
    "run_player",
    "sprint_civi",
];
const PASS: &str = "KICK_floor";
const SHOT: &str = "FIGHTlngkck";

type LoadError = Box<dyn std::error::Error>;

fn file(text: &str) -> Result<PesFile, Pes6Error> {
    text.parse().map_err(Pes6Error::Missing)
}

fn load_player(paths: &GamePaths, pes: &Pes6) -> Result<PlayerAssets, LoadError> {
    let mut vice_city = ViceCity::open(&paths.vice_city)?;
    let tommy = vice_city.load_model("player")?;
    let animations = vice_city.load_animations("ped")?;
    let (model, textures) = pes.load_player(&PlayerParts {
        body: file(PLAYER_BODY)?,
        kit: Some(file(PLAYER_KIT)?),
        boots: Some(file(PLAYER_BOOTS)?),
        head: Some(file(PLAYER_HEAD)?),
        hair: Some(file(PLAYER_HAIR)?),
    })?;
    let retargeted = |name: &str, in_place: bool| -> Result<Animation, LoadError> {
        let source = animations
            .iter()
            .find(|animation| animation.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("{name} absente de ped.ifp"))?;
        Ok(retarget::retarget(
            source,
            &tommy,
            &model,
            retarget::PES6_FROM_VICE_CITY,
            retarget::VICE_CITY_ANIMATION_TO_COMMON,
            in_place,
        )?)
    };
    let stride = |name: &str| -> Result<Stride, LoadError> {
        let moving = retargeted(name, false)?;
        Ok(Stride {
            animation: Arc::new(retargeted(name, true)?),
            speed: ground_speed(&moving),
        })
    };
    let gaits = [
        stride(GAITS[0])?,
        stride(GAITS[1])?,
        stride(GAITS[2])?,
        stride(GAITS[3])?,
        stride(GAITS[4])?,
    ];
    let soles = model
        .meshes
        .iter()
        .filter(|mesh| mesh.skin.is_some())
        .flat_map(|mesh| mesh.positions.iter().map(|p| p[1]))
        .fold(0.0f32, f32::min);
    let pass = Arc::new(retargeted(PASS, true)?);
    let shot = Arc::new(retargeted(SHOT, true)?);
    Ok(PlayerAssets {
        model: Arc::new(model),
        textures,
        gaits,
        pass,
        shot,
        soles,
    })
}

/// Ground speed of a retargeted animation, in m/s: how far its root moves
/// horizontally over one cycle (PES units).
fn ground_speed(animation: &Animation) -> f32 {
    let Some(keys) = animation.tracks.first().map(|track| &track.keys) else {
        return 0.0;
    };
    let (Some(first), Some(last)) = (
        keys.first().and_then(|k| k.translation),
        keys.last().and_then(|k| k.translation),
    ) else {
        return 0.0;
    };
    let travel = Vec2::new(last[0] - first[0], last[2] - first[2]).length();
    travel / retarget::PES6_UNITS_PER_METRE / animation.duration.max(1e-3)
}

fn load_assets(paths: &GamePaths) -> Result<(MatchAssets, PlayerAssets), LoadError> {
    let pes = Pes6::open(&paths.pes6)?;
    let stadium: PesFile = STADIUM.parse().map_err(Pes6Error::Missing)?;
    let (stadium, stadium_textures) = pes.load_scenery(&stadium)?;
    let file = |index: usize| PesFile {
        archive: "0_text".into(),
        index,
        path: Vec::new(),
    };
    let ball_texture = BALLS
        .filter_map(|ball| pes.load_texture(&file(2 * ball + 1)).ok())
        .next()
        .ok_or_else(|| Pes6Error::Missing("aucun ballon lisible dans 0_text.afs".into()))?;
    let ball = pes.load_model(
        &file(ball_texture_model(&ball_texture)),
        Some(&ball_texture.name),
    )?;
    let player = load_player(paths, &pes)?;
    Ok((
        MatchAssets {
            stadium,
            stadium_textures,
            ball,
            ball_texture,
        },
        player,
    ))
}

/// Runs the match.
pub fn run(paths: &GamePaths, capture: CapturePlugin) -> AppExit {
    let (assets, player_assets) = match load_assets(paths) {
        Ok(assets) => assets,
        Err(err) => {
            eprintln!("Chaos FC ne peut pas démarrer : lecture des jeux impossible.\n\n{err}");
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
        .add_plugins((
            BevyBridgePlugin,
            RapierPhysicsPlugin::<NoUserData>::default(),
            crate::HudFontPlugin,
            PlayerPlugin,
            capture,
        ))
        .insert_resource(ClearColor(Color::srgb(0.08, 0.08, 0.10)))
        .insert_resource(GlobalAmbientLight {
            brightness: 700.0,
            ..default()
        })
        .insert_resource(assets)
        .insert_resource(player_assets)
        .add_systems(
            Startup,
            (spawn_stadium, spawn_goals, spawn_ball, spawn_player, setup),
        )
        .add_systems(
            Update,
            (
                ball::apply_forces,
                keep_ball_in_play,
                zoom_camera,
                follow_focus,
            )
                .chain(),
        )
        .run()
}

/// The PES stadium, turned Y up and scaled to metres. Its lighting is in
/// its vertex colours: it is drawn unlit.
fn spawn_stadium(mut spawner: ModelSpawner, assets: Res<MatchAssets>) {
    let (stadium, _) = spawner.spawn(
        &assets.stadium,
        &assets.stadium_textures,
        SpawnOptions {
            unlit: true,
            double_sided: true,
        },
    );
    spawner.commands().entity(stadium).insert(
        Transform::from_rotation(Quat::from_array(retarget::PES6_STADIUM_TO_Y_UP))
            .with_scale(Vec3::splat(1.0 / retarget::PES6_STADIUM_UNITS_PER_METRE)),
    );
}

/// Goals, with the net texture of the stadium.
fn spawn_goals(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Res<MatchAssets>,
) {
    let net = assets
        .stadium_textures
        .iter()
        .find(|texture| texture.name == NET_TEXTURE)
        .map(|texture| images.add(bevy_bridge::texture_image(texture)));
    pitch::spawn_goals(&mut commands, &mut meshes, &mut materials, net);
    pitch::spawn_colliders(&mut commands);
}

/// The model file of a ball texture: the file just before it.
fn ball_texture_model(texture: &Texture) -> usize {
    let index: PesFile = texture
        .name
        .parse()
        .expect("textures are named after their file");
    index.index - 1
}

/// Where the ball is put for a kick-off.
const KICK_OFF: Vec3 = Vec3::new(0.0, ball::RADIUS, 0.0);

/// The PES ball on the centre spot, scaled to the size of a real one.
fn spawn_ball(mut spawner: ModelSpawner, assets: Res<MatchAssets>) {
    let (model, _) = spawner.spawn(
        &assets.ball,
        std::slice::from_ref(&assets.ball_texture),
        SpawnOptions::default(),
    );
    let radius = model_radius(&assets.ball);
    let commands = spawner.commands();
    commands
        .entity(model)
        .insert(Transform::from_scale(Vec3::splat(ball::RADIUS / radius)));
    commands
        .spawn((
            Transform::from_translation(KICK_OFF),
            Visibility::default(),
            ball::body(),
            CameraFocus,
        ))
        .add_child(model);
}

/// Where the player stands at kick-off, facing the goal at +X.
const PLAYER_KICK_OFF: Vec3 = Vec3::new(-1.2, 0.0, 0.3);

fn spawn_player(mut spawner: ModelSpawner, assets: Res<PlayerAssets>) {
    player::spawn(&mut spawner, &assets, PLAYER_KICK_OFF, Vec3::X);
}

/// Largest distance of a vertex from the origin: the radius of a ball.
fn model_radius(model: &Model) -> f32 {
    model
        .meshes
        .iter()
        .flat_map(|mesh| &mesh.positions)
        .map(|p| Vec3::from_array(*p).length())
        .fold(0.0, f32::max)
        .max(1e-3)
}

/// A ball that leaves the stadium (over the walls) goes back to the centre.
fn keep_ball_in_play(mut balls: Query<(&mut Transform, &mut Velocity), With<Ball>>) {
    for (mut transform, mut velocity) in &mut balls {
        let p = transform.translation;
        if p.x.abs() > pitch::BOARD_X + 2.0 || p.z.abs() > pitch::BOARD_Z + 2.0 || p.y < -1.0 {
            transform.translation = KICK_OFF;
            *velocity = Velocity::zero();
        }
    }
}

/// What the camera keeps in view (the ball, once there is one).
#[derive(Component)]
pub struct CameraFocus;

/// The camera in the main stand, on the side of the touchline at +Z, as in
/// the wide view of PES: it slides along the pitch with the focus and
/// leans towards it.
#[derive(Component)]
struct BroadcastCamera {
    /// Point looked at, smoothed.
    target: Vec3,
    /// 1: normal; less is closer.
    zoom: f32,
}

impl BroadcastCamera {
    /// Distance from the target, in metres, and height above it: at the
    /// front of the main stand.
    const DISTANCE: f32 = 44.0;
    const HEIGHT: f32 = 21.0;

    fn transform(&self) -> Transform {
        let offset = Vec3::new(0.0, Self::HEIGHT, Self::DISTANCE) * self.zoom;
        Transform::from_translation(self.target + offset).looking_at(self.target, Vec3::Y)
    }

    /// Where to look for a focus at `focus`: along the pitch with it, but
    /// only half way across, so that the far touchline stays in view.
    fn aim(focus: Vec3) -> Vec3 {
        let half_length = pitch::LENGTH / 2.0 - 12.0;
        let half_width = pitch::WIDTH / 2.0;
        Vec3::new(
            focus.x.clamp(-half_length, half_length),
            0.0,
            focus.z.clamp(-half_width, half_width) * 0.5,
        )
    }
}

fn setup(mut commands: Commands, font: Res<crate::HudFont>) {
    let camera = BroadcastCamera {
        target: Vec3::ZERO,
        zoom: 1.0,
    };
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            // PES films the match with a narrow lens from far.
            fov: 15f32.to_radians(),
            ..default()
        }),
        camera.transform(),
        camera,
    ));
    commands.spawn((
        Text::new(
            "Chaos FC - jalon J7\n\
             Flèches ou ZQSD : courir   Maj gauche : sprint\n\
             K : passe   L : tir (maintenir pour la puissance)\n\
             Manette : stick gauche, gâchette droite, A / croix, B / rond\n\
             Molette : zoom",
        ),
        font.text(15.0),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
    spawn_power_gauge(&mut commands, &font);
}

/// The shot power bar, at the bottom of the screen.
fn spawn_power_gauge(commands: &mut Commands, font: &crate::HudFont) {
    commands.spawn((
        PowerGauge,
        Text::new(""),
        font.text(22.0),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(16.0),
            left: Val::Px(20.0),
            ..default()
        },
    ));
}

fn zoom_camera(scroll: Res<AccumulatedMouseScroll>, mut cameras: Query<&mut BroadcastCamera>) {
    let notches = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 100.0,
    };
    if notches == 0.0 {
        return;
    }
    for mut camera in &mut cameras {
        camera.zoom = (camera.zoom * 0.9f32.powf(notches)).clamp(0.3, 2.0);
    }
}

/// Glides the camera towards its focus.
fn follow_focus(
    time: Res<Time>,
    focus: Query<&GlobalTransform, With<CameraFocus>>,
    mut cameras: Query<(&mut BroadcastCamera, &mut Transform)>,
) {
    let goal = focus
        .single()
        .map(|focus| BroadcastCamera::aim(focus.translation()))
        .unwrap_or(Vec3::ZERO);
    // About 90 % of the way in one second.
    let blend = 1.0 - (-2.3 * time.delta_secs()).exp();
    for (mut camera, mut transform) in &mut cameras {
        camera.target = camera.target.lerp(goal, blend);
        *transform = camera.transform();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_camera_follows_along_the_pitch_but_not_past_the_ends() {
        assert_eq!(
            BroadcastCamera::aim(Vec3::new(10.0, 1.0, 20.0)),
            Vec3::new(10.0, 0.0, 10.0)
        );
        assert_eq!(
            BroadcastCamera::aim(Vec3::new(60.0, 0.0, 0.0)).x,
            pitch::LENGTH / 2.0 - 12.0
        );
    }
}
