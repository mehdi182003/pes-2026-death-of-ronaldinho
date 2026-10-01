//! Command-line tools to dump, inspect and extract game files.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use asset_bridge::cache;
use asset_bridge::config::{self, Game};
use asset_tools::source;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "asset-tools",
    version,
    about = "Outils d'inspection des fichiers de GTA Vice City et de PES 6"
)]
struct Cli {
    /// Fichier de configuration (par défaut : $CHAOS_FC_CONFIG, sinon ./config.toml).
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Vérifie les chemins des jeux déclarés dans le fichier de configuration.
    CheckConfig,

    /// Archive models/gta3.img de Vice City.
    #[command(subcommand)]
    Img(ImgCommand),
}

#[derive(Subcommand)]
enum ImgCommand {
    /// Liste les fichiers de l'archive.
    List {
        /// N'affiche que les noms qui contiennent ce texte (sans tenir compte de la casse).
        #[arg(long)]
        filter: Option<String>,
    },

    /// Extrait des fichiers de l'archive, par défaut dans le dossier de cache.
    Extract {
        /// Noms des fichiers à extraire (par exemple player.dff).
        #[arg(required = true)]
        names: Vec<String>,

        /// Dossier de destination (par défaut : dossier de cache de Chaos FC).
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let config_file = cli.config.unwrap_or_else(config::config_path);
    let result = match cli.command {
        Command::CheckConfig => check_config(&config_file),
        Command::Img(ImgCommand::List { filter }) => img_list(&config_file, filter.as_deref()),
        Command::Img(ImgCommand::Extract { names, out }) => {
            img_extract(&config_file, &names, out.as_deref())
        }
    };
    match result {
        Ok(code) => code,
        Err(err) => {
            eprintln!("Erreur : {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn check_config(config_file: &Path) -> Result<ExitCode> {
    let paths = config::read_paths(config_file)?;

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
    Ok(if all_ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn img_list(config_file: &Path, filter: Option<&str>) -> Result<ExitCode> {
    let archive = source::open_vice_city_img(config_file)?;
    let filter = filter.map(str::to_lowercase);
    let mut shown = 0;
    println!("{:<24} {:>10} {:>12}", "nom", "secteur", "octets");
    for entry in archive.entries() {
        if filter
            .as_ref()
            .is_some_and(|filter| !entry.name.to_lowercase().contains(filter))
        {
            continue;
        }
        println!(
            "{:<24} {:>10} {:>12}",
            entry.name,
            entry.offset_sectors,
            entry.byte_size()
        );
        shown += 1;
    }
    println!(
        "{shown} fichier(s) affiché(s) sur {}",
        archive.entries().len()
    );
    Ok(ExitCode::SUCCESS)
}

fn img_extract(config_file: &Path, names: &[String], out: Option<&Path>) -> Result<ExitCode> {
    let out = match out {
        Some(dir) => dir.to_path_buf(),
        None => cache::extraction_dir(Game::ViceCity)
            .context("dossier de cache introuvable : précisez --out")?,
    };
    std::fs::create_dir_all(&out).with_context(|| format!("création de {}", out.display()))?;

    let mut archive = source::open_vice_city_img(config_file)?;
    for name in names {
        let entry = archive
            .find(name)
            .with_context(|| format!("{name} est absent de models/gta3.img"))?
            .clone();
        let data = archive.read(&entry)?;
        let path = out.join(&entry.name);
        std::fs::write(&path, data).with_context(|| format!("écriture de {}", path.display()))?;
        println!("{}", path.display());
    }
    Ok(ExitCode::SUCCESS)
}
