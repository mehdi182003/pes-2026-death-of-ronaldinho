//! The player's PES 6 install: the AFS archives of its `dat` folder, and
//! what is known of the files they hold.

use std::path::{Path, PathBuf};

use formats_pes::afs::{AfsArchive, AfsError};

#[derive(Debug, thiserror::Error)]
pub enum Pes6Error {
    #[error(transparent)]
    Afs(#[from] AfsError),

    #[error("impossible de lire {} : {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{0}")]
    Missing(String),
}

/// The player's PES 6 install.
pub struct Pes6 {
    dat: PathBuf,
}

impl Pes6 {
    /// Finds the `dat` folder of the install folder `root`.
    pub fn open(root: &Path) -> Result<Self, Pes6Error> {
        let dat = find_ignoring_case(root, "dat").ok_or_else(|| {
            Pes6Error::Missing(format!("dossier dat introuvable dans {}", root.display()))
        })?;
        Ok(Self { dat })
    }

    /// File names of the AFS archives, sorted (e.g. `0_sound.afs`).
    pub fn archive_names(&self) -> Result<Vec<String>, Pes6Error> {
        let entries = std::fs::read_dir(&self.dat).map_err(|source| Pes6Error::Io {
            path: self.dat.clone(),
            source,
        })?;
        let mut names: Vec<String> = entries
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| {
                Path::new(name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("afs"))
            })
            .collect();
        names.sort_by_key(|name| name.to_lowercase());
        Ok(names)
    }

    /// Opens the archive `name` of `dat`, ignoring case; `.afs` is optional.
    pub fn open_archive(&self, name: &str) -> Result<AfsArchive, Pes6Error> {
        let file = if Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("afs"))
        {
            name.to_owned()
        } else {
            format!("{name}.afs")
        };
        let path = find_ignoring_case(&self.dat, &file)
            .ok_or_else(|| Pes6Error::Missing(format!("{file} absent du dossier dat")))?;
        Ok(AfsArchive::open(&path)?)
    }
}

/// Slots of `0_text.afs` (first and last index, both included), from the
/// map published by the PES 6 modding community ("MAP 0_text.afs PES6",
/// obipes6.blogspot.com). Indices start at 0.
// HYPOTHÈSE: drawn up on the full game. Checked on the demo for the faces,
// hairstyles, kits, numbers, palettes and ADX slots (their contents have
// the expected signatures, see docs/formats/afs.md); the others are empty
// or nearly empty in the demo.
const TEXT_MAP: &[(usize, usize, &str)] = &[
    (0, 48, "ballons"),
    (431, 446, "arbitres"),
    (535, 536, "drapeaux et emblèmes"),
    (1891, 2937, "visages"),
    (2938, 3404, "visages (éditeur)"),
    (4448, 4902, "coiffures (éditeur)"),
    (4922, 5316, "coiffures (éditeur)"),
    (5322, 5338, "chaussures"),
    (5339, 5443, "chaussures (éditeur)"),
    (5444, 5455, "palettes"),
    (5456, 5472, "numéros et polices"),
    (5473, 6831, "maillots"),
    (6872, 6912, "sons ADX"),
    (6913, 6914, "foule"),
    (6915, 6939, "panneaux publicitaires"),
];

/// What the community map says slot `index` of archive `archive` holds.
pub fn section(archive: &str, index: usize) -> Option<&'static str> {
    if archive.eq_ignore_ascii_case("0_text.afs") {
        TEXT_MAP
            .iter()
            .find(|(first, last, _)| (*first..=*last).contains(&index))
            .map(|(_, _, name)| *name)
    } else if archive.eq_ignore_ascii_case("0_sound.afs") {
        // Every file of the demo's 0_sound.afs is an ADX sound.
        Some("sons")
    } else {
        None
    }
}

/// Entry of `dir` named `name`, ignoring case.
fn find_ignoring_case(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(name)
        })
        .map(|entry| entry.path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_sections_are_ordered_and_do_not_overlap() {
        for pair in TEXT_MAP.windows(2) {
            assert!(pair[0].0 <= pair[0].1 && pair[0].1 < pair[1].0, "{pair:?}");
        }
        assert_eq!(section("0_text.afs", 1943), Some("visages"));
        assert_eq!(section("0_TEXT.AFS", 5481), Some("maillots"));
        assert_eq!(section("0_text.afs", 57), None);
        assert_eq!(section("0_sound.afs", 3), Some("sons"));
        assert_eq!(section("e_text.afs", 3), None);
    }

    #[test]
    fn lists_and_opens_archives_ignoring_case() {
        let tmp = tempfile::tempdir().unwrap();
        let dat = tmp.path().join("DAT");
        std::fs::create_dir(&dat).unwrap();
        let mut archive = b"AFS\0".to_vec();
        archive.extend([0; 12]);
        std::fs::write(dat.join("0_Text.afs"), &archive).unwrap();
        std::fs::write(dat.join("readme.txt"), b"").unwrap();

        let pes = Pes6::open(tmp.path()).unwrap();
        assert_eq!(pes.archive_names().unwrap(), ["0_Text.afs"]);
        assert!(pes.open_archive("0_text").unwrap().entries().is_empty());
        assert!(matches!(
            pes.open_archive("e_text.afs"),
            Err(Pes6Error::Missing(_))
        ));
    }
}
