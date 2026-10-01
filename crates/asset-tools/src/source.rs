//! Inputs of the tools: a file on disk, or `vc:<name>` for an entry of Vice
//! City's `models/gta3.img`.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result};
use asset_bridge::config::{self, Game};
use formats_rw::img::ImgArchive;

/// Prefix selecting an entry of Vice City's `models/gta3.img`.
pub const VICE_CITY_IMG_PREFIX: &str = "vc:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    File(PathBuf),
    ViceCityImg(String),
}

impl FromStr for Source {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.strip_prefix(VICE_CITY_IMG_PREFIX) {
            Some("") => Err(format!(
                "nom d'entrée manquant après « {VICE_CITY_IMG_PREFIX} »"
            )),
            Some(name) => Ok(Source::ViceCityImg(name.to_owned())),
            None => Ok(Source::File(PathBuf::from(text))),
        }
    }
}

impl Source {
    /// Short name, for titles and messages.
    pub fn label(&self) -> String {
        match self {
            Source::File(path) => path.display().to_string(),
            Source::ViceCityImg(name) => format!("{VICE_CITY_IMG_PREFIX}{name}"),
        }
    }

    /// Reads the whole input. Entries of an IMG archive include their sector
    /// padding.
    pub fn read(&self, config_file: &Path) -> Result<Vec<u8>> {
        match self {
            Source::File(path) => {
                std::fs::read(path).with_context(|| format!("lecture de {}", path.display()))
            }
            Source::ViceCityImg(name) => {
                let mut archive = open_vice_city_img(config_file)?;
                let entry = archive
                    .find(name)
                    .with_context(|| format!("{name} est absent de models/gta3.img"))?
                    .clone();
                Ok(archive.read(&entry)?)
            }
        }
    }
}

/// Opens `models/gta3.img` of the Vice City install declared in
/// `config_file`.
pub fn open_vice_city_img(config_file: &Path) -> Result<ImgArchive> {
    let paths = config::read_paths(config_file)?;
    let vice_city = paths.check(Game::ViceCity)?;
    Ok(ImgArchive::open_pair(
        &vice_city.join("models").join("gta3"),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sources() {
        assert_eq!(
            "vc:player.dff".parse(),
            Ok(Source::ViceCityImg("player.dff".into()))
        );
        assert_eq!(
            "extracted/player.dff".parse(),
            Ok(Source::File("extracted/player.dff".into()))
        );
        assert!("vc:".parse::<Source>().is_err());
    }
}
