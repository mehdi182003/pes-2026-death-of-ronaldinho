//! Command-line tools to dump, inspect and extract game files.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use asset_bridge::cache;
use asset_bridge::config::{self, Game};
use asset_bridge::vice_city;
use asset_tools::dump;
use asset_tools::source::{self, Source};
use clap::{Parser, Subcommand};
use formats_rw::sfx::{self, SoundBank};

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

    /// Banque de sons audio/sfx.SDT et sfx.RAW de Vice City.
    #[command(subcommand)]
    Sfx(SfxCommand),

    /// Dump hexadécimal annoté : arbre des chunks pour un fichier RenderWare (DFF, TXD).
    Dump {
        /// Fichier à lire, ou vc:<nom> pour une entrée de models/gta3.img (ex. vc:player.dff).
        source: Source,

        /// Dump hexadécimal brut, sans analyse RenderWare.
        #[arg(long)]
        raw: bool,

        /// Octets affichés par chunk de données, ou au total en mode brut (0 : tout).
        #[arg(long, default_value_t = 64)]
        bytes: usize,

        /// Mode brut : offset du premier octet affiché.
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
}

#[derive(Subcommand)]
enum SfxCommand {
    /// Liste les sons : numéro, fréquence, durée.
    List {
        /// Premier numéro affiché.
        #[arg(long, default_value_t = 0)]
        from: usize,

        /// Nombre de sons affichés.
        #[arg(long, default_value_t = 100)]
        count: usize,
    },

    /// Exporte des sons en WAV, par défaut dans le dossier de cache.
    Export {
        /// Numéros des sons (par exemple 50 51).
        #[arg(required = true)]
        indices: Vec<usize>,

        /// Dossier de destination (par défaut : dossier de cache de Chaos FC).
        #[arg(long)]
        out: Option<PathBuf>,
    },
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
        Command::Sfx(SfxCommand::List { from, count }) => sfx_list(&config_file, from, count),
        Command::Sfx(SfxCommand::Export { indices, out }) => {
            sfx_export(&config_file, &indices, out.as_deref())
        }
        Command::Dump {
            source,
            raw,
            bytes,
            offset,
        } => dump_source(&config_file, &source, raw, bytes, offset),
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

fn dump_source(
    config_file: &Path,
    source: &Source,
    raw: bool,
    max_bytes: usize,
    offset: usize,
) -> Result<ExitCode> {
    let data = source.read(config_file)?;
    println!("{} ({} octets)", source.label(), data.len());
    if !raw && dump::looks_like_renderware(&data) {
        match dump::renderware_tree(&data, max_bytes) {
            Ok(tree) => {
                print!("{tree}");
                return Ok(ExitCode::SUCCESS);
            }
            Err(err) => eprintln!("Analyse RenderWare impossible ({err}) : dump brut."),
        }
    }
    let start = offset.min(data.len());
    let end = if max_bytes == 0 {
        data.len()
    } else {
        (start + max_bytes).min(data.len())
    };
    print!("{}", dump::hex(&data[start..end], start, ""));
    Ok(ExitCode::SUCCESS)
}

fn open_sound_bank(config_file: &Path) -> Result<SoundBank> {
    let vice_city = config::read_paths(config_file)?.check(Game::ViceCity)?;
    let (sdt, raw) = vice_city::sound_bank_paths(&vice_city)
        .context("audio/sfx.SDT ou audio/sfx.RAW introuvable dans l'installation de Vice City")?;
    Ok(SoundBank::open(&sdt, &raw)?)
}

fn sfx_list(config_file: &Path, from: usize, count: usize) -> Result<ExitCode> {
    let bank = open_sound_bank(config_file)?;
    println!("{:>6} {:>9} {:>9}", "numéro", "Hz", "durée (s)");
    for (index, entry) in bank.entries().iter().enumerate().skip(from).take(count) {
        println!(
            "{index:>6} {:>9} {:>9.3}",
            entry.sample_rate,
            entry.duration()
        );
    }
    println!("{} sons dans la banque", bank.entries().len());
    Ok(ExitCode::SUCCESS)
}

fn sfx_export(config_file: &Path, indices: &[usize], out: Option<&Path>) -> Result<ExitCode> {
    let out = match out {
        Some(dir) => dir.to_path_buf(),
        None => cache::extraction_dir(Game::ViceCity)
            .context("dossier de cache introuvable : précisez --out")?
            .join("sfx"),
    };
    std::fs::create_dir_all(&out).with_context(|| format!("création de {}", out.display()))?;
    let mut bank = open_sound_bank(config_file)?;
    for &index in indices {
        let samples = bank.read_samples(index)?;
        let sample_rate = bank.entries()[index].sample_rate;
        let path = out.join(format!("sfx_{index:04}.wav"));
        std::fs::write(&path, sfx::wav_bytes(sample_rate, &samples))
            .with_context(|| format!("écriture de {}", path.display()))?;
        println!("{}", path.display());
    }
    Ok(ExitCode::SUCCESS)
}
