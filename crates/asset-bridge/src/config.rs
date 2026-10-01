//! Player configuration: where the original games are installed.
//!
//! The player points Chaos FC to their own copies of GTA Vice City and PES 6
//! in `config.toml`. Nothing from those games is ever shipped with Chaos FC.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Environment variable that overrides the location of the configuration file.
pub const CONFIG_ENV_VAR: &str = "CHAOS_FC_CONFIG";

/// Default configuration file, relative to the current directory.
pub const DEFAULT_CONFIG_FILE: &str = "config.toml";

/// Returns the configuration file to use: `$CHAOS_FC_CONFIG` if set and not
/// empty, `./config.toml` otherwise.
pub fn config_path() -> PathBuf {
    std::env::var_os(CONFIG_ENV_VAR)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_FILE))
}

/// An original game whose files Chaos FC reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Game {
    ViceCity,
    Pes6,
}

impl Game {
    pub const ALL: [Game; 2] = [Game::ViceCity, Game::Pes6];

    /// Key of this game in the `[paths]` section of `config.toml`.
    pub fn config_key(self) -> &'static str {
        match self {
            Game::ViceCity => "vice_city",
            Game::Pes6 => "pes6",
        }
    }

    /// Checks that `dir` looks like an install of this game. On failure,
    /// returns a short description of what is missing.
    fn check_layout(self, dir: &Path) -> Result<(), String> {
        match self {
            Game::ViceCity => {
                for file in ["models/gta3.img", "models/gta3.dir"] {
                    if !dir.join(file).is_file() {
                        return Err(format!("fichier {file} introuvable"));
                    }
                }
                Ok(())
            }
            Game::Pes6 => {
                // HYPOTHÈSE: a PC install of PES 6 keeps its AFS archives in a
                // `dat` folder. Not yet checked on a real copy; the exact
                // archive names will be listed during J4.
                let has_afs = fs::read_dir(dir.join("dat"))
                    .map(|entries| {
                        entries.flatten().any(|entry| {
                            entry
                                .path()
                                .extension()
                                .is_some_and(|ext| ext.eq_ignore_ascii_case("afs"))
                        })
                    })
                    .unwrap_or(false);
                if has_afs {
                    Ok(())
                } else {
                    Err("aucune archive .afs dans le dossier dat".to_owned())
                }
            }
        }
    }
}

impl fmt::Display for Game {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Game::ViceCity => "GTA Vice City",
            Game::Pes6 => "PES 6",
        })
    }
}

/// The `[paths]` section, as written by the player.
///
/// Every key is optional so that a missing key produces a clear message
/// rather than a TOML deserialization error.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct PathsSection {
    pub vice_city: Option<PathBuf>,
    pub pes6: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    paths: PathsSection,
}

impl PathsSection {
    /// Path configured for `game`, if any. An empty string counts as missing.
    pub fn get(&self, game: Game) -> Option<&Path> {
        let path = match game {
            Game::ViceCity => &self.vice_city,
            Game::Pes6 => &self.pes6,
        };
        path.as_deref().filter(|path| !path.as_os_str().is_empty())
    }

    /// Checks the install directory of a single game.
    pub fn check(&self, game: Game) -> Result<PathBuf, PathProblem> {
        let path = self.get(game).ok_or(PathProblem::Missing { game })?;
        let path = path.to_path_buf();
        if !path.exists() {
            return Err(PathProblem::DoesNotExist { game, path });
        }
        if !path.is_dir() {
            return Err(PathProblem::NotADirectory { game, path });
        }
        if let Err(missing) = game.check_layout(&path) {
            return Err(PathProblem::NotAnInstall {
                game,
                path,
                missing,
            });
        }
        Ok(path)
    }

    /// Checks both games and reports every problem at once.
    pub fn validate(&self) -> Result<GamePaths, Vec<PathProblem>> {
        match (self.check(Game::ViceCity), self.check(Game::Pes6)) {
            (Ok(vice_city), Ok(pes6)) => Ok(GamePaths { vice_city, pes6 }),
            (vice_city, pes6) => Err([vice_city.err(), pes6.err()]
                .into_iter()
                .flatten()
                .collect()),
        }
    }

    /// Resolves relative paths against `base` (the folder of the config file).
    fn resolve_relative_to(&mut self, base: &Path) {
        for path in [&mut self.vice_city, &mut self.pes6].into_iter().flatten() {
            if path.is_relative() && !path.as_os_str().is_empty() {
                *path = base.join(&*path);
            }
        }
    }
}

/// Validated install directories of both games.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamePaths {
    pub vice_city: PathBuf,
    pub pes6: PathBuf,
}

/// Why the configured directory of a game cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathProblem {
    #[error(
        "[paths] {} : chemin absent. Indiquez le dossier d'installation de {game}.",
        .game.config_key()
    )]
    Missing { game: Game },

    #[error(
        "[paths] {} = \"{}\" : ce dossier n'existe pas.",
        .game.config_key(),
        .path.display()
    )]
    DoesNotExist { game: Game, path: PathBuf },

    #[error(
        "[paths] {} = \"{}\" : ce chemin n'est pas un dossier.",
        .game.config_key(),
        .path.display()
    )]
    NotADirectory { game: Game, path: PathBuf },

    #[error(
        "[paths] {} = \"{}\" : ce dossier ne ressemble pas à une installation de {game} ({missing}).",
        .game.config_key(),
        .path.display()
    )]
    NotAnInstall {
        game: Game,
        path: PathBuf,
        missing: String,
    },
}

/// Why the configuration cannot be loaded.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error(
        "Fichier de configuration introuvable : {}\n\
         Copiez config.example.toml en config.toml et indiquez-y les dossiers \
         d'installation de GTA Vice City et de PES 6.",
        .path.display()
    )]
    NotFound { path: PathBuf },

    #[error("Impossible de lire {} : {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("{} n'est pas un fichier TOML valide :\n{source}", .path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error(
        "Chemins des jeux invalides dans {} :\n{}",
        .path.display(),
        bullet_list(.problems)
    )]
    InvalidPaths {
        path: PathBuf,
        problems: Vec<PathProblem>,
    },
}

fn bullet_list(problems: &[PathProblem]) -> String {
    problems
        .iter()
        .map(|problem| format!("  - {problem}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Reads the `[paths]` section of the configuration file, without checking
/// the directories. Relative paths are resolved against the folder of the
/// configuration file.
pub fn read_paths(config_file: &Path) -> Result<PathsSection, ConfigError> {
    let text = fs::read_to_string(config_file).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            ConfigError::NotFound {
                path: config_file.to_path_buf(),
            }
        } else {
            ConfigError::Io {
                path: config_file.to_path_buf(),
                source,
            }
        }
    })?;
    let mut config: ConfigFile = toml::from_str(&text).map_err(|source| ConfigError::Parse {
        path: config_file.to_path_buf(),
        source,
    })?;
    let base = config_file.parent().unwrap_or(Path::new(""));
    config.paths.resolve_relative_to(base);
    Ok(config.paths)
}

/// Reads the configuration file and checks that both games are installed
/// where it says.
pub fn load_game_paths(config_file: &Path) -> Result<GamePaths, ConfigError> {
    read_paths(config_file)?
        .validate()
        .map_err(|problems| ConfigError::InvalidPaths {
            path: config_file.to_path_buf(),
            problems,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Creates empty placeholder files mimicking an install layout. They
    /// contain no game data.
    fn touch(root: &Path, files: &[&str]) {
        for file in files {
            let path = root.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"").unwrap();
        }
    }

    fn fake_vice_city(root: &Path) -> PathBuf {
        let dir = root.join("vc");
        touch(&dir, &["models/gta3.img", "models/gta3.dir"]);
        dir
    }

    fn fake_pes6(root: &Path) -> PathBuf {
        let dir = root.join("pes6");
        touch(&dir, &["dat/0_text.afs"]);
        dir
    }

    /// Writes `config.toml` in `root` and returns its path.
    fn write_config(root: &Path, contents: &str) -> PathBuf {
        let path = root.join("config.toml");
        fs::write(&path, contents).unwrap();
        path
    }

    fn toml_path(path: &Path) -> String {
        // Literal TOML strings keep Windows backslashes as-is.
        format!("'{}'", path.display())
    }

    #[test]
    fn valid_config_is_accepted() {
        let tmp = TempDir::new().unwrap();
        let vc = fake_vice_city(tmp.path());
        let pes = fake_pes6(tmp.path());
        let config = write_config(
            tmp.path(),
            &format!(
                "[paths]\nvice_city = {}\npes6 = {}\n",
                toml_path(&vc),
                toml_path(&pes)
            ),
        );

        let paths = load_game_paths(&config).unwrap();
        assert_eq!(
            paths,
            GamePaths {
                vice_city: vc,
                pes6: pes
            }
        );
    }

    #[test]
    fn relative_paths_are_resolved_from_the_config_folder() {
        let tmp = TempDir::new().unwrap();
        fake_vice_city(tmp.path());
        fake_pes6(tmp.path());
        let config = write_config(tmp.path(), "[paths]\nvice_city = 'vc'\npes6 = 'pes6'\n");

        let paths = load_game_paths(&config).unwrap();
        assert_eq!(paths.vice_city, tmp.path().join("vc"));
        assert_eq!(paths.pes6, tmp.path().join("pes6"));
    }

    #[test]
    fn missing_config_file_is_reported() {
        let tmp = TempDir::new().unwrap();
        let err = load_game_paths(&tmp.path().join("config.toml")).unwrap_err();
        assert!(matches!(err, ConfigError::NotFound { .. }), "{err:?}");
        assert!(err.to_string().contains("config.example.toml"));
    }

    #[test]
    fn invalid_toml_is_reported() {
        let tmp = TempDir::new().unwrap();
        let config = write_config(tmp.path(), "[paths\nvice_city = ");
        let err = load_game_paths(&config).unwrap_err();
        assert!(matches!(err, ConfigError::Parse { .. }), "{err:?}");
    }

    #[test]
    fn missing_and_empty_paths_are_reported_together() {
        let tmp = TempDir::new().unwrap();
        let config = write_config(tmp.path(), "[paths]\nvice_city = ''\n");
        let err = load_game_paths(&config).unwrap_err();
        let ConfigError::InvalidPaths { problems, .. } = &err else {
            panic!("unexpected error: {err:?}");
        };
        assert_eq!(
            problems,
            &[
                PathProblem::Missing {
                    game: Game::ViceCity
                },
                PathProblem::Missing { game: Game::Pes6 },
            ]
        );
        let message = err.to_string();
        assert!(message.contains("[paths] vice_city"), "{message}");
        assert!(message.contains("[paths] pes6"), "{message}");
    }

    #[test]
    fn config_without_paths_section_reports_both_games() {
        let tmp = TempDir::new().unwrap();
        let config = write_config(tmp.path(), "");
        let err = load_game_paths(&config).unwrap_err();
        let ConfigError::InvalidPaths { problems, .. } = err else {
            panic!("unexpected error: {err:?}");
        };
        assert_eq!(problems.len(), 2);
    }

    #[test]
    fn nonexistent_directory_is_reported() {
        let tmp = TempDir::new().unwrap();
        let section = PathsSection {
            vice_city: Some(tmp.path().join("nowhere")),
            pes6: None,
        };
        assert!(matches!(
            section.check(Game::ViceCity),
            Err(PathProblem::DoesNotExist {
                game: Game::ViceCity,
                ..
            })
        ));
    }

    #[test]
    fn file_instead_of_directory_is_reported() {
        let tmp = TempDir::new().unwrap();
        touch(tmp.path(), &["not-a-dir"]);
        let section = PathsSection {
            vice_city: None,
            pes6: Some(tmp.path().join("not-a-dir")),
        };
        assert!(matches!(
            section.check(Game::Pes6),
            Err(PathProblem::NotADirectory {
                game: Game::Pes6,
                ..
            })
        ));
    }

    #[test]
    fn directory_without_game_files_is_reported() {
        let tmp = TempDir::new().unwrap();
        // An install of the other game is not accepted.
        let section = PathsSection {
            vice_city: Some(fake_pes6(tmp.path())),
            pes6: Some(fake_vice_city(tmp.path())),
        };
        let problems = section.validate().unwrap_err();
        assert_eq!(problems.len(), 2);
        for problem in &problems {
            assert!(
                matches!(problem, PathProblem::NotAnInstall { .. }),
                "{problem:?}"
            );
        }
        assert!(problems[0].to_string().contains("models/gta3.img"));
    }
}
