//! Asset viewer: shows a Vice City model in its bind pose (T-pose for a
//! character), without textures for now (they arrive in J2).
//!
//! ```sh
//! cargo run -p asset-tools --bin viewer -- player
//! ```

use std::f32::consts::FRAC_PI_2;
use std::path::PathBuf;

use anyhow::Result;
use asset_bridge::config::{self, Game};
use asset_bridge::model::Model;
use asset_bridge::vice_city::ViceCity;
use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use clap::Parser;

#[derive(Parser)]
#[command(name = "viewer", about = "Visualiseur de modèles de GTA Vice City")]
struct Args {
    /// Modèle de models/gta3.img, par exemple player ou colt45.
    model: String,

    /// Fichier de configuration (par défaut : $CHAOS_FC_CONFIG, sinon ./config.toml).
    #[arg(long)]
    config: Option<PathBuf>,
}

fn main() -> AppExit {
    let args = Args::parse();
    let model = match load_model(&args) {
        Ok(model) => model,
        Err(err) => {
            eprintln!("Erreur : {err:#}");
            return AppExit::error();
        }
    };
    let scene = ViewerScene::new(model);

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

fn load_model(args: &Args) -> Result<Model> {
    let config_file = args.config.clone().unwrap_or_else(config::config_path);
    let vice_city = config::read_paths(&config_file)?.check(Game::ViceCity)?;
    Ok(ViceCity::open(&vice_city)?.load_model(&args.model)?)
}

/// The model and what the viewer derives from it once.
#[derive(Resource)]
struct ViewerScene {
    model: Model,
    /// World matrix of each node of the model.
    node_world: Vec<Mat4>,
    /// Bind-pose position of each bone, and its parent bone.
    bones: Vec<(Vec3, Option<usize>)>,
    /// Bounds of the vertices, in model space.
    min: Vec3,
    max: Vec3,
}

impl ViewerScene {
    fn new(model: Model) -> Self {
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
) {
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
            let material = StandardMaterial {
                base_color: Color::srgba(r, g, b, a),
                perceptual_roughness: 0.8,
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
    let help = format!(
        "{name} : {vertices} sommets, {triangles} triangles, {bones} os\n\
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
