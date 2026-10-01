//! Asset viewer: shows a textured Vice City model, in its bind pose (T-pose
//! for a character) or playing an animation in a loop, or the textures of a
//! TXD laid flat.
//!
//! ```sh
//! cargo run -p asset-tools --bin viewer -- player
//! cargo run -p asset-tools --bin viewer -- player --anim run_player
//! cargo run -p asset-tools --bin viewer -- --textures D:\ViceCity\txd\LOADSC0.TXD
//! ```

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;
use std::path::PathBuf;

use anyhow::{Context, Result};
use asset_bridge::config::{self, Game};
use asset_bridge::model::{self as neutral, Animation, Model, Texture, Track};
use asset_bridge::vice_city::{self, ViceCity, ViceCityError};
use asset_tools::source::Source;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::transform::TransformSystems;
use clap::Parser;

#[derive(Parser)]
#[command(
    name = "viewer",
    about = "Visualiseur de modèles, d'animations et de textures de GTA Vice City"
)]
struct Args {
    /// Modèle de models/gta3.img, par exemple player ou colt45.
    #[arg(required_unless_present = "textures")]
    model: Option<String>,

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
        .insert_resource(Playback { time: 0.0 })
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (handle_keys, animate, orbit_camera, update_help).chain(),
        )
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

    let name = args.model.as_deref().expect("clap requires a model");
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
            Some(animation)
        }
        None => None,
    };
    Ok(ViewerScene::new(model, textures, false, note, animation))
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
    animation: Option<Animation>,
    /// Node driven by each track of the animation.
    track_nodes: Vec<Option<usize>>,
    /// Move of the root bone over the animation, removed to run in place.
    root_drift: Option<(usize, Vec3)>,
    /// Transform of each node relative to its parent, in the bind pose.
    bind_locals: Vec<Mat4>,
    /// For each bone of the skeleton: its node and its parent bone.
    bones: Vec<(usize, Option<usize>)>,
    /// Bounds of the vertices, in model space.
    min: Vec3,
    max: Vec3,
}

impl ViewerScene {
    fn new(
        model: Model,
        textures: Vec<Texture>,
        flat: bool,
        note: Option<String>,
        animation: Option<Animation>,
    ) -> Self {
        // Bind pose: a bone sits where the inverse of its inverse bind matrix
        // puts it (the frames of a few models are not in bind pose); other
        // nodes keep their frame.
        let bone_of_node = |node: usize| {
            model
                .skeleton
                .as_ref()
                .and_then(|skeleton| skeleton.bones.iter().position(|b| b.node == node))
        };
        let mut bind_world: Vec<Mat4> = Vec::with_capacity(model.nodes.len());
        let mut bind_locals = Vec::with_capacity(model.nodes.len());
        for (index, node) in model.nodes.iter().enumerate() {
            let parent_world = node.parent.map_or(Mat4::IDENTITY, |p| bind_world[p]);
            let world = match (bone_of_node(index), &model.skeleton) {
                (Some(bone), Some(skeleton)) => {
                    Mat4::from_cols_array(&skeleton.bones[bone].inverse_bind).inverse()
                }
                _ => parent_world * Mat4::from_cols_array(&node.local),
            };
            bind_locals.push(parent_world.inverse() * world);
            bind_world.push(world);
        }

        let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for mesh in &model.meshes {
            for &position in &mesh.positions {
                let point = bind_world[mesh.node].transform_point3(Vec3::from_array(position));
                min = min.min(point);
                max = max.max(point);
            }
        }
        if model.vertex_count() == 0 {
            (min, max) = (Vec3::ZERO, Vec3::ZERO);
        }

        // Parent bone: the closest ancestor node that is also a bone.
        let mut bones = Vec::new();
        if let Some(skeleton) = &model.skeleton {
            for bone in &skeleton.bones {
                let mut parent = model.nodes[bone.node].parent;
                let parent_bone = loop {
                    match parent {
                        Some(node) => match bone_of_node(node) {
                            Some(index) => break Some(index),
                            None => parent = model.nodes[node].parent,
                        },
                        None => break None,
                    }
                };
                bones.push((bone.node, parent_bone));
            }
        }

        let track_nodes: Vec<Option<usize>> = animation
            .iter()
            .flat_map(|animation| &animation.tracks)
            .map(|track| track.node_in(&model))
            .collect();
        // The root bone is the first bone of the hierarchy.
        let root_drift = animation.as_ref().and_then(|animation| {
            let root = model.skeleton.as_ref()?.bones.first()?.node;
            let track = track_nodes.iter().position(|&node| node == Some(root))?;
            let keys = &animation.tracks[track].keys;
            let first = Vec3::from_array(keys.first()?.translation?);
            let last = Vec3::from_array(keys.last()?.translation?);
            Some((track, last - first))
        });

        Self {
            model,
            textures,
            flat,
            note,
            animation,
            track_nodes,
            root_drift,
            bind_locals,
            bones,
            min,
            max,
        }
    }

    /// Local transform of every node at time `t`: the bind pose, overridden
    /// by the animation.
    fn pose(&self, t: f32) -> Vec<Transform> {
        let mut locals: Vec<Transform> = self
            .bind_locals
            .iter()
            .map(|&local| Transform::from_matrix(local))
            .collect();
        if let Some(animation) = &self.animation {
            for (index, (track, node)) in animation.tracks.iter().zip(&self.track_nodes).enumerate()
            {
                if let Some(node) = node {
                    let drift = self
                        .root_drift
                        .filter(|&(root, _)| root == index)
                        .map(|(_, drift)| drift * (t / animation.duration.max(1e-3)));
                    apply_sample(&mut locals[*node], track, t, drift);
                }
            }
        }
        locals
    }

    /// Bounds of what is displayed: the vertices in the bind pose, or the
    /// bones over the whole animation (plus a margin for the flesh around
    /// them).
    fn displayed_bounds(&self, root: &Transform) -> (Vec3, Vec3) {
        let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        let root = root.to_matrix();
        if let Some(animation) = &self.animation {
            for step in 0..16 {
                let locals = self.pose(animation.duration * step as f32 / 16.0);
                let mut world: Vec<Mat4> = Vec::with_capacity(locals.len());
                for (node, local) in self.model.nodes.iter().zip(&locals) {
                    let parent = node.parent.map_or(root, |p| world[p]);
                    let matrix = parent * local.to_matrix();
                    let point = matrix.w_axis.truncate();
                    min = min.min(point);
                    max = max.max(point);
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

/// Sets the transform of a bone from its track at time `t`: interpolated
/// between the two surrounding keys. `drift` is subtracted from the
/// translation (root motion removed).
fn apply_sample(transform: &mut Transform, track: &Track, t: f32, drift: Option<Vec3>) {
    let keys = &track.keys;
    let Some(last) = keys.len().checked_sub(1) else {
        return;
    };
    let next = keys
        .iter()
        .position(|key| key.time > t)
        .unwrap_or(keys.len());
    let (a, b, s) = match next {
        0 => (0, 0, 0.0),
        n if n > last => (last, last, 0.0),
        n => {
            let span = keys[n].time - keys[n - 1].time;
            let s = if span > 0.0 {
                (t - keys[n - 1].time) / span
            } else {
                0.0
            };
            (n - 1, n, s)
        }
    };
    let rotation = |i: usize| Quat::from_array(keys[i].rotation).normalize();
    transform.rotation = rotation(a).slerp(rotation(b), s);
    if let (Some(from), Some(to)) = (keys[a].translation, keys[b].translation) {
        let translation = Vec3::from_array(from).lerp(Vec3::from_array(to), s);
        transform.translation = translation - drift.unwrap_or(Vec3::ZERO);
    }
    if let (Some(from), Some(to)) = (keys[a].scale, keys[b].scale) {
        transform.scale = Vec3::from_array(from).lerp(Vec3::from_array(to), s);
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
            Transform::from_rotation(Quat::from_rotation_x(-FRAC_PI_2))
        } else {
            Transform::IDENTITY
        }
    }
}

#[derive(Resource)]
struct Playback {
    /// Seconds into the animation.
    time: f32,
}

/// Entity of each node of the model.
#[derive(Resource)]
struct NodeEntities(Vec<Entity>);

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

fn setup(
    mut commands: Commands,
    scene: Res<ViewerScene>,
    options: Res<Options>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
) {
    // Materials name their texture; the game ignores case.
    let textures: HashMap<String, (Handle<Image>, bool)> = scene
        .textures
        .iter()
        .map(|texture| {
            let handle = images.add(texture_image(texture));
            (texture.name.to_lowercase(), (handle, texture.is_opaque()))
        })
        .collect();

    let root_transform = options.root_transform();
    let root = commands
        .spawn((ModelRoot, root_transform, Visibility::default()))
        .id();

    // One entity per node, in the bind pose; the animation moves them.
    let mut nodes: Vec<Entity> = Vec::with_capacity(scene.model.nodes.len());
    for (node, local) in scene.model.nodes.iter().zip(&scene.bind_locals) {
        let parent = node.parent.map_or(root, |p| nodes[p]);
        nodes.push(
            commands
                .spawn((
                    Transform::from_matrix(*local),
                    Visibility::default(),
                    ChildOf(parent),
                ))
                .id(),
        );
    }
    let skin = scene.model.skeleton.as_ref().map(|skeleton| SkinnedMesh {
        inverse_bindposes: bindposes.add(SkinnedMeshInverseBindposes::from(
            skeleton
                .bones
                .iter()
                .map(|bone| Mat4::from_cols_array(&bone.inverse_bind))
                .collect::<Vec<_>>(),
        )),
        joints: skeleton.bones.iter().map(|bone| nodes[bone.node]).collect(),
    });

    for mesh in &scene.model.meshes {
        for primitive in &mesh.primitives {
            let mut bevy_mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, mesh.positions.clone())
            .with_inserted_indices(Indices::U32(primitive.indices.clone()));
            match &mesh.normals {
                Some(normals) => {
                    bevy_mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals.clone());
                }
                None => bevy_mesh.compute_smooth_normals(),
            }
            if let Some(uvs) = &mesh.uvs {
                bevy_mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs.clone());
            }
            if let Some(colors) = &mesh.colors {
                // Bevy expects linear vertex colours.
                let linear: Vec<[f32; 4]> = colors
                    .iter()
                    .map(|&[r, g, b, a]| Color::srgba(r, g, b, a).to_linear().to_f32_array())
                    .collect();
                bevy_mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, linear);
            }
            let skinned = mesh.skin.as_ref().zip(skin.as_ref());
            if let Some((mesh_skin, _)) = skinned {
                bevy_mesh.insert_attribute(
                    Mesh::ATTRIBUTE_JOINT_INDEX,
                    VertexAttributeValues::Uint16x4(mesh_skin.joints.clone()),
                );
                bevy_mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, mesh_skin.weights.clone());
            }

            let [r, g, b, a] = primitive.material.base_color;
            let texture = primitive
                .material
                .texture
                .as_ref()
                .and_then(|name| textures.get(&name.to_lowercase()));
            let material = StandardMaterial {
                base_color: Color::srgba(r, g, b, a),
                base_color_texture: texture.map(|(handle, _)| handle.clone()),
                alpha_mode: match texture {
                    Some((_, false)) => AlphaMode::Mask(0.5),
                    _ => AlphaMode::Opaque,
                },
                perceptual_roughness: 0.8,
                unlit: scene.flat,
                cull_mode: if scene.flat {
                    None
                } else {
                    StandardMaterial::default().cull_mode
                },
                ..default()
            };
            let mut entity = commands.spawn((
                Mesh3d(meshes.add(bevy_mesh)),
                MeshMaterial3d(materials.add(material)),
            ));
            match skinned {
                // Skinned vertices follow the joints; the bounds computed
                // from the bind pose would wrongly cull the animated mesh.
                Some((_, skin)) => entity.insert((skin.clone(), NoFrustumCulling, ChildOf(root))),
                None => entity.insert((Transform::IDENTITY, ChildOf(nodes[mesh.node]))),
            };
        }
    }
    commands.insert_resource(NodeEntities(nodes));

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

/// Bevy image of a decoded texture.
fn texture_image(texture: &Texture) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        texture.rgba8.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    // HYPOTHÈSE: Vice City textures repeat (sampler 0x1106 of the DFF and
    // TXD files, read as wrap/wrap after the GTAMods wiki).
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

fn handle_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut options: ResMut<Options>,
    scene: Res<ViewerScene>,
    mut roots: Query<&mut Transform, (With<ModelRoot>, Without<Orbit>)>,
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
        for mut transform in &mut roots {
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

/// Plays the animation in a loop, in place.
fn animate(
    time: Res<Time>,
    scene: Res<ViewerScene>,
    options: Res<Options>,
    mut playback: ResMut<Playback>,
    nodes: Res<NodeEntities>,
    mut transforms: Query<&mut Transform, (Without<ModelRoot>, Without<Orbit>)>,
) {
    let Some(animation) = &scene.animation else {
        return;
    };
    if options.playing {
        playback.time = (playback.time + time.delta_secs()) % animation.duration.max(1e-3);
    }
    for (node, local) in scene.pose(playback.time).into_iter().enumerate() {
        if let Ok(mut transform) = transforms.get_mut(nodes.0[node]) {
            *transform = local;
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
    nodes: Option<Res<NodeEntities>>,
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
    let Some(nodes) = nodes else {
        return;
    };
    if options.skeleton {
        let bone_color = Color::srgb(1.0, 0.75, 0.1);
        let position = |node: usize| {
            globals
                .get(nodes.0[node])
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
