//! `--capture <file.png>`: saves a picture of the window once the scene is
//! on screen, then quits. Lets the result be checked without playing.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, save_to_disk};

/// Frame at which the picture is taken: meshes and textures are on screen,
/// and the ball has had time to settle.
const FRAME: u32 = 240;

pub struct CapturePlugin(pub Option<PathBuf>);

impl Plugin for CapturePlugin {
    fn build(&self, app: &mut App) {
        if let Some(path) = &self.0 {
            app.insert_resource(Capture {
                path: path.clone(),
                frame: 0,
            })
            .add_systems(Update, capture);
        }
    }
}

#[derive(Resource)]
struct Capture {
    path: PathBuf,
    frame: u32,
}

fn capture(
    mut commands: Commands,
    mut capture: ResMut<Capture>,
    mut exits: MessageWriter<AppExit>,
) {
    capture.frame += 1;
    if capture.frame == FRAME {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(capture.path.clone()));
    } else if capture.frame == FRAME + 30 {
        exits.write(AppExit::Success);
    }
}
