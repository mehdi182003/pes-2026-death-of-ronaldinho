//! Chaos FC: the Bevy application.

use asset_bridge::config::{self, GamePaths};
use bevy::prelude::*;

/// Install directories of the original games, validated at startup.
#[derive(Resource)]
struct Installs(GamePaths);

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

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Chaos FC".into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(Installs(paths))
        .add_systems(Startup, setup)
        .run()
}

fn setup(mut commands: Commands, installs: Res<Installs>) {
    let GamePaths { vice_city, pes6 } = &installs.0;
    info!("GTA Vice City : {}", vice_city.display());
    info!("PES 6 : {}", pes6.display());

    commands.spawn(Camera2d);
    commands.spawn((
        Text::new(format!(
            "Chaos FC\n\nGTA Vice City : {}\nPES 6 : {}",
            vice_city.display(),
            pes6.display()
        )),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(16.0),
            left: Val::Px(16.0),
            ..default()
        },
    ));
}
