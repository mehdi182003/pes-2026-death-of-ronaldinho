//! Asset viewer: shows a textured Vice City model, in its bind pose (T-pose
//! for a character) or playing an animation in a loop, or the textures of a
//! TXD laid flat. A PES 6 model can be shown alone or next to it.
//!
//! ```sh
//! cargo run -p asset-tools --bin viewer -- player
//! cargo run -p asset-tools --bin viewer -- player --anim run_player
//! cargo run -p asset-tools --bin viewer -- --textures D:\ViceCity\txd\LOADSC0.TXD
//! cargo run -p asset-tools --bin viewer -- player --pes 0_text:431 --pes-texture 0_text:432
//! ```

use std::f32::consts::FRAC_PI_2;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use asset_bridge::config::{self, Game};
use asset_bridge::model::{self as neutral, Animation, Model, Texture};
use asset_bridge::pes6::{Pes6, PesFile};
use asset_bridge::vice_city::{self, ViceCity, ViceCityError};
use asset_tools::source::Source;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_bridge::{
    AnimationLayer, AnimationLayers, BevyBridgePlugin, BindPose, ModelSpawner, SpawnOptions,
    SpawnedModel, bone_links,
};
use clap::Parser;

#[derive(Parser)]
#[command(
    name = "viewer",
    about = "Visualiseur de modèles, d'animations et de textures de GTA Vice City et de PES 6"
)]
struct Args {
    /// Modèle de models/gta3.img, par exemple player ou colt45.
    #[arg(required_unless_present_any = ["textures", "pes"])]
    model: Option<String>,

    /// Modèle de PES 6, <archive>:<numéro>[/<sous-fichier>] (par exemple
    /// 0_text:431) : seul, ou à côté du modèle de Vice City.
    #[arg(long, conflicts_with_all = ["textures", "anim"])]
    pes: Option<PesFile>,

    /// Texture du modèle de PES 6 (par exemple 0_text:432).
    #[arg(long, requires = "pes")]
    pes_texture: Option<PesFile>,

    /// Dictionnaire de textures du modèle (par défaut : celui du même nom).
    #[arg(long)]
    txd: Option<String>,

    /// Animation à jouer en boucle, par exemple run_player.
    #[arg(long, conflicts_with = "textures")]
    anim: Option<String>,

    /// Paquet d'animations : ped (anim/ped.ifp, par défaut) ou un IFP de gta3.img.
    #[arg(long, default_value = "ped")]
    anim_package: String,

    /// Affiche à plat les textures d'un TXD : vc:<nom> ou chemin d'un fichier.
    #[arg(long, conflicts_with = "model")]
    textures: Option<Source>,

    /// Fichier de configuration (par défaut : $CHAOS_FC_CONFIG, sinon ./config.toml).
    #[arg(long)]
    config: Option<PathBuf>,
}

fn main() -> AppExit {
    let args = Args::parse();
    let scene = match load(&args) {
        Ok(scene) => scene,
        Err(err) => {
            eprintln!("Erreur : {err:#}");
            return AppExit::error();
        }
    };
    let title = match &scene.animation {
        Some(animation) => format!("{} - {}", scene.model.name, animation.name),
        None => scene.model.name.clone(),
    };
    // Animations stand the character along +Z, the bind pose along +Y.
    let z_up = scene.animation.is_some();

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("Chaos FC - visualiseur - {title}"),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(BevyBridgePlugin)
        .insert_resource(ClearColor(Color::srgb(0.16, 0.17, 0.20)))
        .insert_resource(GlobalAmbientLight {
            brightness: 400.0,
            ..default()
        })
        .insert_resource(scene)
        .insert_resource(Options {
            skeleton: true,
            mesh: true,
            grid: true,
            z_up,
            playing: true,
        })
        .add_systems(Startup, setup)
        .add_systems(Update, (handle_keys, orbit_camera, update_help).chain())
        // After transform propagation, so that the skeleton follows the
        // skinned mesh of the same frame.
        .add_systems(PostUpdate, draw_gizmos.after(TransformSystems::Propagate))
        .run()
}

fn load(args: &Args) -> Result<ViewerScene> {
    let config_file = args.config.clone().unwrap_or_else(config::config_path);
    if let Some(source) = &args.textures {
        let textures = vice_city::decode_txd(&source.label(), &source.read(&config_file)?)?;
        let board = texture_board(&source.label(), &textures);
        return Ok(ViewerScene::new(board, textures, true, None, None));
    }

    let pes_scene = match &args.pes {
        Some(file) => Some(load_pes(&config_file, file, args.pes_texture.as_ref())?),
        None => None,
    };
    let Some(name) = args.model.as_deref() else {
        let (model, textures) = pes_scene.expect("clap requires a model or --pes");
        return Ok(ViewerScene::new(model, textures, false, None, None));
    };
    let vice_city = config::read_paths(&config_file)?.check(Game::ViceCity)?;
    let mut game = ViceCity::open(&vice_city)?;
    let model = game.load_model(name)?;
    let txd = args
        .txd
        .as_deref()
        .unwrap_or_else(|| name.trim_end_matches(".dff").trim_end_matches(".DFF"));
    let (textures, note) = match game.load_textures(txd) {
        Ok(textures) => (textures, None),
        // A model without a dictionary of the same name stays untextured.
        Err(ViceCityError::NotFound(file)) => (Vec::new(), Some(format!("{file} introuvable"))),
        Err(err) => return Err(err.into()),
    };
    let animation = match &args.anim {
        Some(wanted) => {
            let animations = game.load_animations(&args.anim_package)?;
            let animation = animations
                .into_iter()
                .find(|animation| animation.name.eq_ignore_ascii_case(wanted))
                .with_context(|| format!("{wanted} absente de {}", args.anim_package))?;
            Some(Arc::new(animation))
        }
        None => None,
    };
    if let Some((pes_model, pes_textures)) = pes_scene {
        let (model, mut textures) = (beside(model, pes_model), textures);
        textures.extend(pes_textures);
        return Ok(ViewerScene::new(model, textures, false, note, None));
    }
    Ok(ViewerScene::new(model, textures, false, note, animation))
}

/// A PES 6 model and its texture, the model scaled to metres.
fn load_pes(
    config_file: &std::path::Path,
    file: &PesFile,
    texture: Option<&PesFile>,
) -> Result<(Model, Vec<Texture>)> {
    let pes6 = config::read_paths(config_file)?.check(Game::Pes6)?;
    let pes = Pes6::open(&pes6)?;
    let textures = match texture {
        Some(texture) => vec![pes.load_texture(texture)?],
        None => Vec::new(),
    };
    let mut model = pes.load_model(file, textures.first().map(|t| t.name.as_str()))?;
    // PES models are Y-up like the viewer; only the units change.
    let scale = Mat4::from_scale(Vec3::splat(1.0 / retarget::PES6_UNITS_PER_METRE));
    model.nodes[0].local = (scale * Mat4::from_cols_array(&model.nodes[0].local)).to_cols_array();
    Ok((model, textures))
}

/// `base` with `other` added one metre to its right (+X), feet at the same
/// height, both in their bind pose.
fn beside(mut base: Model, other: Model) -> Model {
    let lowest = |model: &Model| {
        let bind = BindPose::of(model);
        model
            .meshes
            .iter()
            .flat_map(|mesh| {
                let world = bind.worlds[mesh.node];
                mesh.positions
                    .iter()
                    .map(move |&p| world.transform_point3(Vec3::from_array(p)).y)
            })
            .fold(f32::MAX, f32::min)
    };
    let lift = lowest(&base) - lowest(&other);
    let shift = base.nodes.len() + 1;
    base.nodes.push(neutral::Node {
        name: other.name.clone(),
        parent: None,
        local: Mat4::from_translation(Vec3::new(1.0, lift, 0.0)).to_cols_array(),
        bone_id: None,
    });
    for node in other.nodes {
        base.nodes.push(neutral::Node {
            parent: Some(node.parent.map_or(shift - 1, |parent| parent + shift)),
            ..node
        });
    }
    base.meshes
        .extend(other.meshes.into_iter().map(|mesh| neutral::Mesh {
            node: mesh.node + shift,
            ..mesh
        }));
    base.name = format!("{} + {}", base.name, other.name);
    base
}

/// Lays textures side by side on upright quads one metre high, so that they
/// can be checked by eye.
fn texture_board(name: &str, textures: &[Texture]) -> Model {
    let mut x = 0.0;
    let meshes = textures
        .iter()
        .map(|texture| {
            let width = texture.width as f32 / texture.height as f32;
            let mesh = neutral::Mesh {
                node: 0,
                positions: vec![
                    [x, 0.0, 0.0],
                    [x + width, 0.0, 0.0],
                    [x + width, 1.0, 0.0],
                    [x, 1.0, 0.0],
                ],
                normals: Some(vec![[0.0, 0.0, 1.0]; 4]),
                // The top-left corner of the image goes to the top-left
                // corner of the quad.
                uvs: Some(vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]),
                colors: None,
                primitives: vec![neutral::Primitive {
                    material: neutral::Material {
                        base_color: [1.0; 4],
                        texture: Some(texture.name.clone()),
                    },
                    indices: vec![0, 1, 2, 0, 2, 3],
                }],
                skin: None,
            };
            x += width + 0.1;
            mesh
        })
        .collect();
    Model {
        name: name.to_owned(),
        nodes: vec![neutral::Node {
            name: "board".into(),
            parent: None,
            local: Mat4::IDENTITY.to_cols_array(),
            bone_id: None,
        }],
        meshes,
        skeleton: None,
    }
}

/// The model and what the viewer derives from it once.
#[derive(Resource)]
struct ViewerScene {
    model: Model,
    textures: Vec<Texture>,
    /// Texture board: no lighting, both faces visible.
    flat: bool,
    /// Shown in the help, e.g. a missing texture dictionary.
    note: Option<String>,
    animation: Option<Arc<Animation>>,
    /// For each bone: its node and its parent bone.
    bones: Vec<(usize, Option<usize>)>,
    /// Bounds of the vertices in the bind pose, in model space.
    min: Vec3,
    max: Vec3,
}

impl ViewerScene {
    fn new(
        model: Model,
        textures: Vec<Texture>,
        flat: bool,
        note: Option<String>,
        animation: Option<Arc<Animation>>,
    ) -> Self {
        let bind = BindPose::of(&model);
        let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for mesh in &model.meshes {
            for &position in &mesh.positions {
                let point = bind.worlds[mesh.node].transform_point3(Vec3::from_array(position));
                min = min.min(point);
                max = max.max(point);
            }
        }
        if model.vertex_count() == 0 {
            (min, max) = (Vec3::ZERO, Vec3::ZERO);
        }
        Self {
            bones: bone_links(&model),
            model,
            textures,
            flat,
            note,
            animation,
            min,
            max,
        }
    }

    /// Bounds of what is displayed: the vertices in the bind pose, or the
    /// nodes over the whole animation (plus a margin for the flesh around
    /// the bones).
    fn displayed_bounds(&self, root: &Transform) -> (Vec3, Vec3) {
        let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        let root = root.to_matrix();
        if let Some(animation) = &self.animation {
            let layer = AnimationLayer::new(animation.clone(), &self.model, true);
            let bind: Vec<Transform> = BindPose::of(&self.model)
                .locals
                .iter()
                .map(|&local| Transform::from_matrix(local))
                .collect();
            for step in 0..16 {
                let mut locals = bind.clone();
                layer.apply_at(animation.duration * step as f32 / 16.0, &mut locals);
                let mut world: Vec<Mat4> = Vec::with_capacity(locals.len());
                for (node, local) in self.model.nodes.iter().zip(&locals) {
                    let matrix = node.parent.map_or(root, |p| world[p]) * local.to_matrix();
                    min = min.min(matrix.w_axis.truncate());
                    max = max.max(matrix.w_axis.truncate());
                    world.push(matrix);
                }
            }
            return (min - Vec3::splat(0.15), max + Vec3::splat(0.15));
        }
        for x in [self.min.x, self.max.x] {
            for y in [self.min.y, self.max.y] {
                for z in [self.min.z, self.max.z] {
                    let corner = root.transform_point3(Vec3::new(x, y, z));
                    min = min.min(corner);
                    max = max.max(corner);
                }
            }
        }
        (min, max)
    }
}

#[derive(Resource)]
struct Options {
    skeleton: bool,
    mesh: bool,
    grid: bool,
    /// What is shown stands along +Z (world objects, animated characters)
    /// rather than +Y (characters in bind pose): turn it so that it stands
    /// up in the viewer.
    z_up: bool,
    playing: bool,
}

impl Options {
    fn root_transform(&self) -> Transform {
        if self.z_up {
            Transform::from_rotation(Quat::from_array(retarget::VICE_CITY_TO_Y_UP))
        } else {
            Transform::IDENTITY
        }
    }
}

#[derive(Component)]
struct ModelRoot;

#[derive(Component)]
struct HelpText;

#[derive(Component)]
struct Orbit {
    target: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
}

impl Orbit {
    /// Frames bounds seen slightly from above, three-quarters.
    fn framing(min: Vec3, max: Vec3) -> Self {
        Self {
            target: (min + max) / 2.0,
            yaw: 0.6,
            pitch: -0.25,
            distance: (max - min).length().max(0.5) * 1.3,
        }
    }

    fn transform(&self) -> Transform {
        let rotation = Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0);
        Transform::from_translation(self.target + rotation * Vec3::new(0.0, 0.0, self.distance))
            .looking_at(self.target, Vec3::Y)
    }
}

fn setup(mut spawner: ModelSpawner, scene: Res<ViewerScene>, options: Res<Options>) {
    let root_transform = options.root_transform();
    let (root, _) = spawner.spawn(
        &scene.model,
        &scene.textures,
        SpawnOptions {
            unlit: scene.flat,
            double_sided: scene.flat,
        },
    );
    let commands = spawner.commands();
    commands.entity(root).insert((ModelRoot, root_transform));
    if let Some(animation) = &scene.animation {
        let layer = AnimationLayer::new(animation.clone(), &scene.model, true).looping();
        commands.entity(root).insert(AnimationLayers(vec![layer]));
    }

    let (min, max) = scene.displayed_bounds(&root_transform);
    let orbit = Orbit::framing(min, max);
    commands.spawn((Camera3d::default(), orbit.transform(), orbit));
    commands.spawn((
        DirectionalLight {
            illuminance: 6000.0,
            ..default()
        },
        Transform::from_xyz(2.0, 4.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        HelpText,
        Text::new(""),
        TextFont::from_font_size(15.0),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
}

fn handle_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut options: ResMut<Options>,
    scene: Res<ViewerScene>,
    mut roots: Query<(&mut Transform, Option<&mut AnimationLayers>), With<ModelRoot>>,
    mut meshes: Query<&mut Visibility, With<Mesh3d>>,
    mut cameras: Query<&mut Orbit>,
) {
    // Letters at the same place on AZERTY and QWERTY keyboards.
    if keys.just_pressed(KeyCode::KeyS) {
        options.skeleton = !options.skeleton;
    }
    if keys.just_pressed(KeyCode::KeyG) {
        options.grid = !options.grid;
    }
    if keys.just_pressed(KeyCode::Space) && scene.animation.is_some() {
        options.playing = !options.playing;
        for (_, layers) in &mut roots {
            for layer in layers
                .into_iter()
                .flat_map(|layers| layers.into_inner().0.iter_mut())
            {
                layer.paused = !options.playing;
            }
        }
    }
    if keys.just_pressed(KeyCode::KeyV) {
        options.mesh = !options.mesh;
        for mut visibility in &mut meshes {
            *visibility = if options.mesh {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
    let turn = keys.just_pressed(KeyCode::KeyH);
    if turn {
        options.z_up = !options.z_up;
        for (mut transform, _) in &mut roots {
            *transform = options.root_transform();
        }
    }
    if turn || keys.just_pressed(KeyCode::KeyF) {
        let (min, max) = scene.displayed_bounds(&options.root_transform());
        for mut orbit in &mut cameras {
            *orbit = Orbit::framing(min, max);
        }
    }
}

fn orbit_camera(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut cameras: Query<(&mut Orbit, &mut Transform)>,
) {
    let Ok((mut orbit, mut transform)) = cameras.single_mut() else {
        return;
    };
    if buttons.pressed(MouseButton::Left) {
        orbit.yaw -= motion.delta.x * 0.008;
        orbit.pitch = (orbit.pitch - motion.delta.y * 0.008).clamp(-1.5, 1.5);
    }
    let notches = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 100.0,
    };
    if notches != 0.0 {
        orbit.distance = (orbit.distance * 0.9f32.powf(notches)).clamp(0.1, 100.0);
    }
    *transform = orbit.transform();
}

fn draw_gizmos(
    mut gizmos: Gizmos,
    scene: Res<ViewerScene>,
    options: Res<Options>,
    models: Query<&SpawnedModel, With<ModelRoot>>,
    globals: Query<&GlobalTransform>,
) {
    if options.grid {
        // Floor under the feet, cells of 25 cm.
        let (min, max) = scene.displayed_bounds(&options.root_transform());
        let center = (min + max) / 2.0;
        gizmos.grid(
            Isometry3d::new(
                Vec3::new(center.x, min.y, center.z),
                Quat::from_rotation_x(FRAC_PI_2),
            ),
            UVec2::splat(16),
            Vec2::splat(0.25),
            Color::srgba(1.0, 1.0, 1.0, 0.15),
        );
    }
    let Ok(model) = models.single() else {
        return;
    };
    if options.skeleton {
        let bone_color = Color::srgb(1.0, 0.75, 0.1);
        let position = |node: usize| {
            globals
                .get(model.nodes[node])
                .map(|global| global.translation())
                .ok()
        };
        for &(node, parent) in &scene.bones {
            let Some(joint) = position(node) else {
                continue;
            };
            gizmos.sphere(Isometry3d::from_translation(joint), 0.012, bone_color);
            if let Some(parent) = parent.and_then(|bone| position(scene.bones[bone].0)) {
                gizmos.line(joint, parent, bone_color);
            }
        }
    }
}

fn update_help(
    scene: Res<ViewerScene>,
    options: Res<Options>,
    mut texts: Query<&mut Text, With<HelpText>>,
) {
    if !options.is_changed() {
        return;
    }
    let on_off = |on: bool| if on { "oui" } else { "non" };
    let size = scene.max - scene.min;
    let model = &scene.model;
    let textures: Vec<String> = scene
        .textures
        .iter()
        .map(|t| format!("{} ({}x{})", t.name, t.width, t.height))
        .collect();
    let texture_line = match (&scene.note, textures.len()) {
        (Some(note), _) => format!("Textures : aucune ({note})"),
        (None, 0) => "Textures : aucune".to_owned(),
        (None, count) if count <= 6 => format!("Textures : {}", textures.join(", ")),
        (None, count) => format!("Textures : {count}, dont {}", textures[..6].join(", ")),
    };
    let animation_line = match &scene.animation {
        Some(animation) => format!(
            "Animation : {} ({:.2} s, en boucle sur place)   [Espace] {}\n",
            animation.name,
            animation.duration,
            if options.playing { "pause" } else { "lecture" }
        ),
        None => String::new(),
    };
    let help = format!(
        "{name} : {vertices} sommets, {triangles} triangles, {bones} os\n\
         {texture_line}\n\
         {animation_line}\
         Taille dans le fichier : X {x:.2} m, Y {y:.2} m, Z {z:.2} m\n\
         Souris : clic gauche + glisser pour tourner, molette pour zoomer\n\
         [S] squelette : {skeleton}   [V] maillage : {mesh}   [G] grille : {grid}\n\
         [H] axe vertical : {axis}   [F] recadrer",
        name = model.name,
        vertices = model.vertex_count(),
        triangles = model.triangle_count(),
        bones = scene.bones.len(),
        x = size.x,
        y = size.y,
        z = size.z,
        skeleton = on_off(options.skeleton),
        mesh = on_off(options.mesh),
        grid = on_off(options.grid),
        axis = if options.z_up { "Z" } else { "Y" },
    );
    for mut text in &mut texts {
        text.0 = help.clone();
    }
}
