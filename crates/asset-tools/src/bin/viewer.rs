//! Asset viewer: shows a textured Vice City model in its bind pose (T-pose
//! for a character), or the textures of a TXD laid flat.
//!
//! ```sh
//! cargo run -p asset-tools --bin viewer -- player
//! cargo run -p asset-tools --bin viewer -- --textures D:\ViceCity\txd\LOADSC0.TXD
//! ```

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;
use std::path::PathBuf;

use anyhow::Result;
use asset_bridge::config::{self, Game};
use asset_bridge::model::{self as neutral, Model, Texture};
use asset_bridge::vice_city::{self, ViceCity, ViceCityError};
use asset_tools::source::Source;
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use clap::Parser;

#[derive(Parser)]
#[command(
    name = "viewer",
    about = "Visualiseur de modèles et de textures de GTA Vice City"
)]
struct Args {
    /// Modèle de models/gta3.img, par exemple player ou colt45.
    #[arg(required_unless_present = "textures")]
    model: Option<String>,

    /// Dictionnaire de textures du modèle (par défaut : celui du même nom).
    #[arg(long)]
    txd: Option<String>,

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

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("Chaos FC - visualiseur - {}", scene.model.name),
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
            z_up: false,
        })
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (handle_keys, orbit_camera, draw_gizmos, update_help).chain(),
        )
        .run()
}

fn load(args: &Args) -> Result<ViewerScene> {
    let config_file = args.config.clone().unwrap_or_else(config::config_path);
    if let Some(source) = &args.textures {
        let textures = vice_city::decode_txd(&source.label(), &source.read(&config_file)?)?;
        let board = texture_board(&source.label(), &textures);
        return Ok(ViewerScene::new(board, textures, true, None));
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
    Ok(ViewerScene::new(model, textures, false, note))
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
    /// World matrix of each node of the model.
    node_world: Vec<Mat4>,
    /// Bind-pose position of each bone, and its parent bone.
    bones: Vec<(Vec3, Option<usize>)>,
    /// Bounds of the vertices, in model space.
    min: Vec3,
    max: Vec3,
}

impl ViewerScene {
    fn new(model: Model, textures: Vec<Texture>, flat: bool, note: Option<String>) -> Self {
        let mut node_world: Vec<Mat4> = Vec::with_capacity(model.nodes.len());
        for node in &model.nodes {
            let local = Mat4::from_cols_array(&node.local);
            let world = match node.parent {
                Some(parent) => node_world[parent] * local,
                None => local,
            };
            node_world.push(world);
        }

        let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for mesh in &model.meshes {
            for &position in &mesh.positions {
                let point = node_world[mesh.node].transform_point3(Vec3::from_array(position));
                min = min.min(point);
                max = max.max(point);
            }
        }
        if model.vertex_count() == 0 {
            (min, max) = (Vec3::ZERO, Vec3::ZERO);
        }

        // The bind pose of a bone is the inverse of its inverse bind matrix.
        // Its parent is the closest ancestor node that is also a bone.
        let mut bones = Vec::new();
        if let Some(skeleton) = &model.skeleton {
            let bone_of_node = |node: usize| skeleton.bones.iter().position(|b| b.node == node);
            for bone in &skeleton.bones {
                let position = Mat4::from_cols_array(&bone.inverse_bind)
                    .inverse()
                    .w_axis
                    .truncate();
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
                bones.push((position, parent_bone));
            }
        }

        Self {
            model,
            textures,
            flat,
            note,
            node_world,
            bones,
            min,
            max,
        }
    }

    /// Bounds once the model is turned as displayed.
    fn displayed_bounds(&self, root: &Transform) -> (Vec3, Vec3) {
        let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for x in [self.min.x, self.max.x] {
            for y in [self.min.y, self.max.y] {
                for z in [self.min.z, self.max.z] {
                    let corner = root.transform_point(Vec3::new(x, y, z));
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
    /// The file stands along +Z (world objects) rather than +Y (characters
    /// in bind pose): turn it so that it stands up in the viewer.
    z_up: bool,
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
            commands.spawn((
                Mesh3d(meshes.add(bevy_mesh)),
                MeshMaterial3d(materials.add(material)),
                Transform::from_matrix(scene.node_world[mesh.node]),
                ChildOf(root),
            ));
        }
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

fn draw_gizmos(mut gizmos: Gizmos, scene: Res<ViewerScene>, options: Res<Options>) {
    let root = options.root_transform();
    if options.grid {
        // Floor under the feet, cells of 25 cm.
        let (min, max) = scene.displayed_bounds(&root);
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
    if options.skeleton {
        let bone_color = Color::srgb(1.0, 0.75, 0.1);
        for &(position, parent) in &scene.bones {
            let joint = root.transform_point(position);
            gizmos.sphere(Isometry3d::from_translation(joint), 0.012, bone_color);
            if let Some(parent) = parent {
                gizmos.line(
                    joint,
                    root.transform_point(scene.bones[parent].0),
                    bone_color,
                );
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
    let help = format!(
        "{name} : {vertices} sommets, {triangles} triangles, {bones} os\n\
         {texture_line}\n\
         Taille dans le fichier : X {x:.2} m, Y {y:.2} m, Z {z:.2} m\n\
         Souris : clic gauche + glisser pour tourner, molette pour zoomer\n\
         [S] squelette : {skeleton}   [V] maillage : {mesh}   [G] grille : {grid}\n\
         [H] axe vertical du fichier : {axis}   [F] recadrer",
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
