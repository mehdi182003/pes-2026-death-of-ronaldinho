//! Command-line tools to dump, inspect and extract game files.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use asset_bridge::cache;
use asset_bridge::config::{self, Game};
use asset_bridge::pes6::{self, Pes6};
use asset_bridge::vice_city;
use asset_tools::dump;
use asset_tools::source::{self, Source};
use clap::{Parser, Subcommand};
use formats_pes::content::{self, Packing, Report};
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

    /// Archives AFS du dossier dat de PES 6.
    #[command(subcommand)]
    Afs(AfsCommand),

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
enum AfsCommand {
    /// Résumé de toutes les archives : nombre de fichiers par contenu.
    Summary,

    /// Liste les fichiers d'une archive : numéro, taille, stockage, contenu,
    /// et section de la carte communautaire.
    List {
        /// Archive du dossier dat (par exemple 0_text ou 0_text.afs).
        archive: String,

        /// Affiche aussi les emplacements vides.
        #[arg(long)]
        empty: bool,

        /// Détaille les sous-fichiers des conteneurs.
        #[arg(long)]
        tree: bool,

        /// N'affiche que les fichiers dont le contenu contient ce texte (par exemple texture).
        #[arg(long)]
        kind: Option<String>,
    },

    /// Extrait des fichiers décompressés, et leurs sous-fichiers, par défaut
    /// dans le dossier de cache.
    Extract {
        /// Archive du dossier dat (par exemple 0_text).
        archive: String,

        /// Numéros des fichiers (par exemple 1943 5452).
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
        Command::Afs(AfsCommand::Summary) => afs_summary(&config_file),
        Command::Afs(AfsCommand::List {
            archive,
            empty,
            tree,
            kind,
        }) => afs_list(&config_file, &archive, empty, tree, kind.as_deref()),
        Command::Afs(AfsCommand::Extract {
            archive,
            indices,
            out,
        }) => afs_extract(&config_file, &archive, &indices, out.as_deref()),
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

fn open_pes6(config_file: &Path) -> Result<Pes6> {
    let pes6 = config::read_paths(config_file)?.check(Game::Pes6)?;
    Ok(Pes6::open(&pes6)?)
}

fn packing_label(packing: &Packing) -> &'static str {
    match packing {
        Packing::Plain => "brut",
        Packing::Stored => "stocké",
        Packing::Compressed => "zlib",
        Packing::Unreadable(_) => "illisible",
    }
}

/// What a report shows in a list: its kind, plus the kinds of the
/// sub-files of a container (e.g. « conteneur : 2 × modèle, 1 × texture »).
fn content_label(report: &Report) -> String {
    if let Packing::Unreadable(reason) = &report.packing {
        return format!("illisible ({reason})");
    }
    if report.children.is_empty() {
        return report.kind.label();
    }
    let mut counts: Vec<(String, usize)> = Vec::new();
    for child in &report.children {
        let label = content_label(child);
        match counts.iter_mut().find(|(known, _)| *known == label) {
            Some((_, count)) => *count += 1,
            None => counts.push((label, 1)),
        }
    }
    let parts: Vec<String> = counts
        .iter()
        .map(|(label, count)| format!("{count} × {label}"))
        .collect();
    format!("{} : {}", report.kind.label(), parts.join(", "))
}

fn print_tree(report: &Report, depth: usize) {
    for (position, child) in report.children.iter().enumerate() {
        println!(
            "{:>width$}.{position:<3} {:>9} {:<9} {}",
            "",
            child.size,
            packing_label(&child.packing),
            content_label(child),
            width = 6 + 4 * depth,
        );
        print_tree(child, depth + 1);
    }
}

fn afs_list(
    config_file: &Path,
    archive_name: &str,
    show_empty: bool,
    tree: bool,
    kind: Option<&str>,
) -> Result<ExitCode> {
    let pes = open_pes6(config_file)?;
    let mut archive = pes.open_archive(archive_name)?;
    let file_name = archive
        .path()
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let kind = kind.map(str::to_lowercase);
    let entries = archive.entries().to_vec();
    let mut shown = 0;
    println!(
        "{:>6} {:>9} {:<9} contenu [section de la carte]",
        "numéro", "octets", "stockage"
    );
    for entry in &entries {
        if entry.is_empty() {
            if show_empty && kind.is_none() {
                println!("{:>6} {:>9} {:<9} -", entry.index, 0, "vide");
                shown += 1;
            }
            continue;
        }
        let report = content::inspect(&archive.read(entry)?);
        let label = content_label(&report);
        if kind
            .as_ref()
            .is_some_and(|kind| !label.to_lowercase().contains(kind))
        {
            continue;
        }
        let section = pes6::section(&file_name, entry.index)
            .map(|name| format!(" [{name}]"))
            .unwrap_or_default();
        println!(
            "{:>6} {:>9} {:<9} {label}{section}",
            entry.index,
            entry.size,
            packing_label(&report.packing)
        );
        if tree {
            print_tree(&report, 0);
        }
        shown += 1;
    }
    let used = entries.iter().filter(|entry| !entry.is_empty()).count();
    println!(
        "{shown} fichier(s) affiché(s) ; {used} emplacement(s) utilisé(s) sur {}",
        entries.len()
    );
    Ok(ExitCode::SUCCESS)
}

fn afs_summary(config_file: &Path) -> Result<ExitCode> {
    let pes = open_pes6(config_file)?;
    for name in pes.archive_names()? {
        let mut archive = pes.open_archive(&name)?;
        let entries = archive.entries().to_vec();
        let mut used = 0;
        let mut unreadable = 0;
        let mut top: Vec<(String, usize)> = Vec::new();
        let mut leaves: Vec<(String, usize)> = Vec::new();
        let count = |counts: &mut Vec<(String, usize)>, label: String| match counts
            .iter_mut()
            .find(|(known, _)| *known == label)
        {
            Some((_, n)) => *n += 1,
            None => counts.push((label, 1)),
        };
        for entry in entries.iter().filter(|entry| !entry.is_empty()) {
            used += 1;
            let report = content::inspect(&archive.read(entry)?);
            if matches!(report.packing, Packing::Unreadable(_)) {
                unreadable += 1;
                count(&mut top, "illisible".into());
                continue;
            }
            count(&mut top, report.kind.label());
            for sub in report.walk() {
                if sub.children.is_empty() && !std::ptr::eq(sub, &report) {
                    let label = match sub.packing {
                        Packing::Unreadable(_) => "illisible".into(),
                        _ => sub.kind.label(),
                    };
                    count(&mut leaves, label);
                }
            }
        }
        println!(
            "{name} : {} emplacements, {used} utilisés, {} vides, {unreadable} illisible(s)",
            entries.len(),
            entries.len() - used
        );
        top.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
        leaves.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
        println!("  fichiers :");
        for (label, n) in &top {
            println!("    {n:>6} × {label}");
        }
        if !leaves.is_empty() {
            println!("  sous-fichiers (dans les conteneurs) :");
            for (label, n) in &leaves {
                println!("    {n:>6} × {label}");
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn afs_extract(
    config_file: &Path,
    archive_name: &str,
    indices: &[usize],
    out: Option<&Path>,
) -> Result<ExitCode> {
    let pes = open_pes6(config_file)?;
    let mut archive = pes.open_archive(archive_name)?;
    let stem = archive
        .path()
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let out = match out {
        Some(dir) => dir.to_path_buf(),
        None => cache::extraction_dir(Game::Pes6)
            .context("dossier de cache introuvable : précisez --out")?
            .join(&stem),
    };
    std::fs::create_dir_all(&out).with_context(|| format!("création de {}", out.display()))?;
    for &index in indices {
        let entry = *archive
            .entries()
            .get(index)
            .with_context(|| format!("{stem}.afs n'a pas de fichier n° {index}"))?;
        if entry.is_empty() {
            println!("n° {index} : emplacement vide");
            continue;
        }
        for file in content::extract(&archive.read(&entry)?) {
            let mut name = format!("{stem}_{index:05}");
            for position in &file.path {
                name.push_str(&format!("_{position}"));
            }
            let path = out.join(format!("{name}.{}", file.kind.extension()));
            std::fs::write(&path, &file.data)
                .with_context(|| format!("écriture de {}", path.display()))?;
            println!("{} ({})", path.display(), file.kind.label());
        }
    }
    Ok(ExitCode::SUCCESS)
}
