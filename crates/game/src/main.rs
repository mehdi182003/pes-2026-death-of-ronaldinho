//! Chaos FC: the Bevy application.
//!
//! - `chaos-fc`: the football match (milestone J7: the pitch, the ball, one
//!   player and goals that count);
//! - `chaos-fc tir`: the shooting range of milestone J3, Tommy and his
//!   weapons.
//!
//! `--capture <file.png>` saves a picture of the window, then quits.

mod ball;
mod capture;
mod football;
mod goals;
mod pitch;
mod player;
mod range;
mod shooting;

use asset_bridge::config;
use bevy::prelude::*;

/// The font of the texts on screen, loaded once at start-up.
#[derive(Resource, Clone)]
struct HudFont(Handle<Font>);

/// A font of the system that has the accented letters of French; the font
/// built into Bevy lacks some. Bevy's is used when none is found.
const SYSTEM_FONTS: &[&str] = &[
    "C:/Windows/Fonts/segoeui.ttf",
    "C:/Windows/Fonts/arial.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
];

/// Loads [`HudFont`] before the scenes are set up.
struct HudFontPlugin;

impl Plugin for HudFontPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_hud_font);
    }
}

fn load_hud_font(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    let handle = SYSTEM_FONTS
        .iter()
        .find_map(|path| std::fs::read(path).ok())
        .map(|bytes| fonts.add(Font::from_bytes(bytes)))
        .unwrap_or_default();
    commands.insert_resource(HudFont(handle));
}

impl HudFont {
    fn text(&self, size: f32) -> TextFont {
        TextFont {
            font: self.0.clone().into(),
            ..TextFont::from_font_size(size)
        }
    }
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
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let capture = match args.iter().position(|arg| arg == "--capture") {
        Some(at) if at + 1 < args.len() => {
            let path = args.remove(at + 1);
            args.remove(at);
            Some(std::path::PathBuf::from(path))
        }
        Some(_) => {
            eprintln!("--capture attend le nom d'un fichier PNG.");
            return AppExit::error();
        }
        None => None,
    };
    let capture = capture::CapturePlugin(capture);
    match args.first().map(String::as_str) {
        None => football::run(&paths, capture),
        Some("tir") => range::run(&paths, capture),
        Some(other) => {
            eprintln!(
                "Mode inconnu « {other} ». Lancez Chaos FC sans argument pour le match, \
                 ou avec « tir » pour le stand de tir."
            );
            AppExit::error()
        }
    }
}
