//! Reading the family name out of a font file, and fonts the system scan skips.

use cosmic_text::fontdb::{self, FaceInfo, Language, Source};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A name of the font (`id` 1 = family, 6 = PostScript name ...) that is not empty: the English Windows one if there
/// is one. Some old fonts have empty names on the "Unicode" platform and the real ones on the Windows platform.
fn name_of(face: &ttf_parser::Face, id: u16) -> Option<String> {
    let names: Vec<_> = face
        .names()
        .into_iter()
        .filter(|n| n.name_id == id && n.is_unicode())
        .filter_map(|n| n.to_string().map(|s| (n.language_id, s)))
        .filter(|(_, s)| !s.trim().is_empty())
        .collect();
    names
        .iter()
        .find(|(lang, _)| *lang == 0x0409)
        .or(names.first())
        .map(|(_, s)| s.clone())
}

/// The family name written inside a font file: the name to put in `TextStyle::font` to get that font.
/// `None` when the file cannot be read or is not a font.
pub fn family_of_file(path: &Path) -> Option<String> {
    let data = std::fs::read(path).ok()?;
    let face = ttf_parser::Face::parse(&data, 0).ok()?;
    // the "typographic family" (id 16) is what the font list shows when there is one, else the plain family (id 1)
    name_of(&face, 16).or_else(|| name_of(&face, 1))
}

/// How tall one line of this font is as a multiple of its size: (ascender - descender) / units per em. Editors that
/// size text by line height (CapCut does) show a font with a small ratio bigger than one with a large ratio.
pub fn line_ratio_of_file(path: &Path) -> Option<f32> {
    let data = std::fs::read(path).ok()?;
    let face = ttf_parser::Face::parse(&data, 0).ok()?;
    let height = f32::from(face.ascender()) - f32::from(face.descender());
    let ratio = height / f32::from(face.units_per_em());
    (ratio.is_finite() && ratio > 0.3).then_some(ratio)
}

fn path_key(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/").to_lowercase()
}

/// Put the installed font files that the scan of the system fonts skipped into the database. (Some old Sinhala fonts,
/// like 4u-Basuru, have empty names on the "Unicode" platform; the scan then adds no face at all, although Windows
/// and every other program use the font fine.)
pub(crate) fn register_skipped(db: &mut fontdb::Database) {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(
            PathBuf::from(d)
                .join("Microsoft")
                .join("Windows")
                .join("Fonts"),
        );
    }
    if let Some(d) = std::env::var_os("WINDIR") {
        dirs.push(PathBuf::from(d).join("Fonts"));
    }
    let known: HashSet<String> = db
        .faces()
        .filter_map(|f| match &f.source {
            Source::File(p) | Source::SharedFile(p, _) => Some(path_key(p)),
            Source::Binary(_) => None,
        })
        .collect();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_font = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("ttf") || e.eq_ignore_ascii_case("otf"));
            if !is_font || known.contains(&path_key(&path)) {
                continue;
            }
            let Ok(data) = std::fs::read(&path) else {
                continue;
            };
            let Ok(face) = ttf_parser::Face::parse(&data, 0) else {
                continue;
            };
            let Some(family) = name_of(&face, 16).or_else(|| name_of(&face, 1)) else {
                continue;
            };
            tracing::info!(
                "font {} was skipped by the scan; added as {family}",
                path.display()
            );
            db.push_face_info(FaceInfo {
                id: fontdb::ID::dummy(),
                source: Source::File(path),
                index: 0,
                post_script_name: name_of(&face, 6).unwrap_or_else(|| family.clone()),
                families: vec![(family, Language::English_UnitedStates)],
                style: if face.is_italic() {
                    fontdb::Style::Italic
                } else {
                    fontdb::Style::Normal
                },
                weight: fontdb::Weight(face.weight().to_number()),
                stretch: fontdb::Stretch::Normal,
                monospaced: face.is_monospaced(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bundled_font_tells_its_family() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts");
        let Some(file) = std::fs::read_dir(&dir).ok().and_then(|d| {
            d.flatten()
                .find(|e| e.path().extension().is_some_and(|x| x == "ttf"))
        }) else {
            return;
        };
        let name = family_of_file(&file.path()).expect("a bundled font has a name");
        assert!(!name.is_empty());
    }

    #[test]
    fn a_missing_or_broken_file_gives_none() {
        assert_eq!(family_of_file(Path::new("no/such/font.ttf")), None);
        let broken = std::env::temp_dir().join("vidzcut-not-a-font.ttf");
        std::fs::write(&broken, b"this is not a font").unwrap();
        assert_eq!(family_of_file(&broken), None);
    }

    /// Every readable font in this user's fonts folder is known to the engine by its file (skipped when there is no
    /// such folder).
    #[test]
    fn every_installed_font_file_is_known_to_the_engine() {
        let Some(local) = std::env::var_os("LOCALAPPDATA") else {
            return;
        };
        let dir = PathBuf::from(local)
            .join("Microsoft")
            .join("Windows")
            .join("Fonts");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        let mut engine = crate::TextEngine::new();
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| !x.eq_ignore_ascii_case("ttf")) {
                continue;
            }
            if family_of_file(&p).is_some() {
                assert!(
                    engine.has_face_file(&p),
                    "{} is not known to the engine",
                    p.display()
                );
            }
        }
    }
}

#[cfg(test)]
mod diagnostics {
    /// Lists the faces of this computer whose file name contains `VZ_FONT` (a diagnostic, run with --ignored).
    #[test]
    #[ignore = "looks at this computer's fonts"]
    fn show_faces_by_file_name() {
        let word = std::env::var("VZ_FONT").unwrap_or_default().to_lowercase();
        let mut engine = crate::TextEngine::new();
        for (path, families) in engine.faces_for_diagnostics() {
            if path.to_lowercase().contains(&word) {
                println!("{path} -> {families:?}");
            }
        }
    }
}

#[cfg(test)]
mod diagnostics_metrics {
    /// Prints the vertical measures of font files (`VZ_FONT_FILES`, separated by `;`).
    #[test]
    #[ignore = "looks at font files"]
    fn show_font_metrics() {
        let Ok(files) = std::env::var("VZ_FONT_FILES") else {
            return;
        };
        for file in files.split(';') {
            let data = std::fs::read(file).unwrap();
            let face = ttf_parser::Face::parse(&data, 0).unwrap();
            let upm = f32::from(face.units_per_em());
            println!(
                "{file}: upm {upm} ascender {} descender {} gap {} -> (asc-desc)/upm {:.3}; typo {:?}/{:?}; cap {:?} x {:?}",
                face.ascender(),
                face.descender(),
                face.line_gap(),
                f32::from(face.ascender() - face.descender()) / upm,
                face.typographic_ascender(),
                face.typographic_descender(),
                face.capital_height(),
                face.x_height(),
            );
        }
    }
}
