//! Command-line tools to dump, inspect and extract game files, plus an asset
//! viewer.

use std::path::PathBuf;
use std::process::ExitCode;

use asset_bridge::config::{self, Game};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "asset-tools",
    version,
    about = "Outils d'inspection des fichiers de GTA Vice City et de PES 6"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Vérifie les chemins des jeux déclarés dans le fichier de configuration.
    CheckConfig {
        /// Fichier de configuration (par défaut : $CHAOS_FC_CONFIG, sinon ./config.toml).
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::CheckConfig { config } => check_config(config.unwrap_or_else(config::config_path)),
    }
}

fn check_config(config_file: PathBuf) -> ExitCode {
    let paths = match config::read_paths(&config_file) {
        Ok(paths) => paths,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };

    println!("Configuration : {}", config_file.display());
    let mut all_ok = true;
    for game in Game::ALL {
        match paths.check(game) {
            Ok(dir) => println!("  [OK]     {game} : {}", dir.display()),
            Err(problem) => {
                all_ok = false;
                println!("  [ERREUR] {problem}");
            }
        }
    }

    if all_ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
