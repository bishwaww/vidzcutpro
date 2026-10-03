//! Look-up tables ("LUTs"): a colour look stored as a cube of colours. Reads `.cube` files, and makes a few of our own.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use vz_core::LutSource;

/// A 3D table: for every red, green and blue on a grid, the colour to show instead.
#[derive(Clone, Debug, PartialEq)]
pub struct Lut3 {
    pub size: usize,
    /// `size³` colours, red changing fastest, then green, then blue.
    pub data: Vec<[f32; 3]>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LutError {
    Empty,
    BadLine(String),
    WrongCount { expected: usize, found: usize },
}

impl std::fmt::Display for LutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "the file has no colour table"),
            Self::BadLine(l) => write!(f, "cannot read the line \"{l}\""),
            Self::WrongCount { expected, found } => {
                write!(f, "expected {expected} colours, found {found}")
            }
        }
    }
}

impl Lut3 {
    /// The table that changes nothing.
    pub fn identity(size: usize) -> Self {
        let mut data = Vec::with_capacity(size * size * size);
        let max = (size - 1) as f32;
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    data.push([r as f32 / max, g as f32 / max, b as f32 / max]);
                }
            }
        }
        Self { size, data }
    }

    /// Make a table from a function of the colour.
    pub fn from_fn(size: usize, f: impl Fn([f32; 3]) -> [f32; 3]) -> Self {
        let mut lut = Self::identity(size);
        for c in &mut lut.data {
            *c = f(*c).map(|v| v.clamp(0.0, 1.0));
        }
        lut
    }

    /// The colour a table gives for `rgb` (each 0 to 1), blending between the nearest grid points.
    pub fn sample(&self, rgb: [f32; 3]) -> [f32; 3] {
        let n = self.size;
        let max = (n - 1) as f32;
        let p = rgb.map(|v| v.clamp(0.0, 1.0) * max);
        let i0 = p.map(|v| v.floor() as usize);
        let i1 = i0.map(|v| (v + 1).min(n - 1));
        let t = [
            p[0] - i0[0] as f32,
            p[1] - i0[1] as f32,
            p[2] - i0[2] as f32,
        ];
        let at = |r: usize, g: usize, b: usize| self.data[(b * n + g) * n + r];
        let mut out = [0.0; 3];
        for (c, o) in out.iter_mut().enumerate() {
            let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
            let c00 = lerp(at(i0[0], i0[1], i0[2])[c], at(i1[0], i0[1], i0[2])[c], t[0]);
            let c10 = lerp(at(i0[0], i1[1], i0[2])[c], at(i1[0], i1[1], i0[2])[c], t[0]);
            let c01 = lerp(at(i0[0], i0[1], i1[2])[c], at(i1[0], i0[1], i1[2])[c], t[0]);
            let c11 = lerp(at(i0[0], i1[1], i1[2])[c], at(i1[0], i1[1], i1[2])[c], t[0]);
            *o = lerp(lerp(c00, c10, t[1]), lerp(c01, c11, t[1]), t[2]);
        }
        out
    }

    /// Read the text of a `.cube` file (3D tables; a 1D table is turned into a 3D one that applies it to each channel).
    pub fn parse_cube(text: &str) -> Result<Self, LutError> {
        let mut size3 = None;
        let mut size1 = None;
        let mut domain = ([0.0f32; 3], [1.0f32; 3]);
        let mut rows: Vec<[f32; 3]> = Vec::new();
        for raw in text.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut words = line.split_whitespace();
            let first = words.next().unwrap_or("");
            match first {
                "TITLE" => {}
                "LUT_3D_SIZE" => {
                    size3 = words.next().and_then(|w| w.parse::<usize>().ok());
                }
                "LUT_1D_SIZE" => {
                    size1 = words.next().and_then(|w| w.parse::<usize>().ok());
                }
                "DOMAIN_MIN" | "DOMAIN_MAX" => {
                    let v: Vec<f32> = words.filter_map(|w| w.parse().ok()).collect();
                    if v.len() == 3 {
                        if first == "DOMAIN_MIN" {
                            domain.0 = [v[0], v[1], v[2]];
                        } else {
                            domain.1 = [v[0], v[1], v[2]];
                        }
                    }
                }
                _ => {
                    let nums: Vec<f32> = line
                        .split_whitespace()
                        .map_while(|w| w.parse().ok())
                        .collect();
                    if nums.len() != 3 {
                        return Err(LutError::BadLine(line.to_owned()));
                    }
                    rows.push([nums[0], nums[1], nums[2]]);
                }
            }
        }
        let scale = |c: [f32; 3]| {
            [
                (c[0] - domain.0[0]) / (domain.1[0] - domain.0[0]).max(1e-6),
                (c[1] - domain.0[1]) / (domain.1[1] - domain.0[1]).max(1e-6),
                (c[2] - domain.0[2]) / (domain.1[2] - domain.0[2]).max(1e-6),
            ]
        };
        if let Some(n) = size3 {
            if n < 2 {
                return Err(LutError::Empty);
            }
            if rows.len() != n * n * n {
                return Err(LutError::WrongCount {
                    expected: n * n * n,
                    found: rows.len(),
                });
            }
            return Ok(Self {
                size: n,
                data: rows.into_iter().map(scale).collect(),
            });
        }
        if let Some(n) = size1 {
            if n < 2 || rows.len() != n {
                return Err(LutError::WrongCount {
                    expected: n,
                    found: rows.len(),
                });
            }
            // a table per channel: build a 3D table from it
            let lookup = |v: f32, c: usize| {
                let p = v.clamp(0.0, 1.0) * (n - 1) as f32;
                let (i, t) = (p.floor() as usize, p - p.floor());
                let j = (i + 1).min(n - 1);
                scale(rows[i])[c] * (1.0 - t) + scale(rows[j])[c] * t
            };
            return Ok(Self::from_fn(17, |rgb| {
                [lookup(rgb[0], 0), lookup(rgb[1], 1), lookup(rgb[2], 2)]
            }));
        }
        Err(LutError::Empty)
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        Self::parse_cube(&text).map_err(|e| e.to_string())
    }
}

/// The looks that come with vidzcut (made by us, as formulas).
pub fn builtin(name: &str) -> Option<Lut3> {
    let lum = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    let table = match name {
        // shadows pushed to teal, highlights to orange
        "teal_orange" => Lut3::from_fn(17, move |c| {
            let l = lum(c);
            let shadow = (1.0 - l).powi(2);
            let high = l * l;
            [
                c[0] + 0.10 * high - 0.08 * shadow,
                c[1] + 0.02 * high + 0.02 * shadow,
                c[2] - 0.10 * high + 0.10 * shadow,
            ]
        }),
        // washed out, lifted, slightly green-yellow
        "faded_film" => Lut3::from_fn(17, move |c| {
            [
                0.08 + c[0] * 0.86 + 0.02,
                0.08 + c[1] * 0.86 + 0.01,
                0.10 + c[2] * 0.80,
            ]
        }),
        // strong contrast with crushed colour
        "bleach_bypass" => Lut3::from_fn(17, move |c| {
            let l = lum(c);
            let mixed = [
                c[0] * 0.5 + l * 0.5,
                c[1] * 0.5 + l * 0.5,
                c[2] * 0.5 + l * 0.5,
            ];
            let contrast = |v: f32| ((v - 0.5) * 1.35 + 0.5).clamp(0.0, 1.0);
            [contrast(mixed[0]), contrast(mixed[1]), contrast(mixed[2])]
        }),
        // channels pushed apart: cyan shadows, yellow highlights
        "cross_process" => Lut3::from_fn(17, move |c| {
            let s = |v: f32, a: f32, b: f32| (v * v * (3.0 - 2.0 * v)) * a + v * b;
            [
                s(c[0], 0.6, 0.4),
                s(c[1], 0.2, 0.8),
                0.15 + c[2] * 0.7 - s(c[2], 0.0, 0.0),
            ]
        }),
        _ => return None,
    };
    Some(table)
}

/// The names of the builtin looks.
pub const BUILTIN_LOOKS: [&str; 4] = [
    "teal_orange",
    "faded_film",
    "bleach_bypass",
    "cross_process",
];

/// Remembers loaded tables (a `.cube` file is read once, not on every frame).
#[derive(Default)]
pub struct LutCache {
    tables: HashMap<String, Option<Arc<Lut3>>>,
}

impl LutCache {
    pub fn get(&mut self, source: &LutSource) -> Option<Arc<Lut3>> {
        let key = match source {
            LutSource::Builtin(name) => format!("builtin:{name}"),
            LutSource::File(path) => format!("file:{}", path.display()),
        };
        self.tables
            .entry(key)
            .or_insert_with(|| match source {
                LutSource::Builtin(name) => builtin(name).map(Arc::new),
                LutSource::File(path) => match Lut3::load(path) {
                    Ok(l) => Some(Arc::new(l)),
                    Err(e) => {
                        tracing::warn!("cannot use LUT {}: {e}", path.display());
                        None
                    }
                },
            })
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identity_table_changes_nothing() {
        let lut = Lut3::identity(9);
        for c in [[0.0, 0.0, 0.0], [1.0, 0.5, 0.25], [0.33, 0.66, 0.99]] {
            let out = lut.sample(c);
            for i in 0..3 {
                assert!((out[i] - c[i]).abs() < 1e-4, "{c:?} -> {out:?}");
            }
        }
    }

    #[test]
    fn a_cube_file_is_read() {
        // a 2x2x2 table that swaps red and blue
        let mut text = String::from("TITLE \"swap\"\n# a comment\nLUT_3D_SIZE 2\n");
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    text.push_str(&format!("{b} {g} {r}\n"));
                }
            }
        }
        let lut = Lut3::parse_cube(&text).unwrap();
        assert_eq!(lut.size, 2);
        let out = lut.sample([1.0, 0.0, 0.0]);
        assert!(
            (out[0] - 0.0).abs() < 1e-5 && (out[2] - 1.0).abs() < 1e-5,
            "{out:?}"
        );
    }

    #[test]
    fn broken_files_are_refused_with_a_reason() {
        assert_eq!(
            Lut3::parse_cube("hello"),
            Err(LutError::BadLine("hello".into()))
        );
        assert_eq!(Lut3::parse_cube(""), Err(LutError::Empty));
        assert_eq!(
            Lut3::parse_cube("LUT_3D_SIZE 2\n0 0 0\n"),
            Err(LutError::WrongCount {
                expected: 8,
                found: 1
            })
        );
    }

    #[test]
    fn a_one_dimensional_table_is_applied_to_each_channel() {
        let text = "LUT_1D_SIZE 2\n0 0 0\n0.5 0.5 0.5\n"; // halves everything
        let lut = Lut3::parse_cube(text).unwrap();
        let out = lut.sample([1.0, 0.5, 0.0]);
        assert!(
            (out[0] - 0.5).abs() < 0.02 && (out[1] - 0.25).abs() < 0.02 && out[2].abs() < 0.02,
            "{out:?}"
        );
    }

    #[test]
    fn the_domain_lines_rescale_the_values() {
        let text = "LUT_3D_SIZE 2\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 2 2 2\n0 0 0\n2 0 0\n0 2 0\n2 2 0\n0 0 2\n2 0 2\n0 2 2\n2 2 2\n";
        let lut = Lut3::parse_cube(text).unwrap();
        let out = lut.sample([1.0, 1.0, 1.0]);
        assert!((out[0] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn our_own_looks_exist_and_stay_in_range() {
        for name in BUILTIN_LOOKS {
            let lut = builtin(name).unwrap_or_else(|| panic!("{name}"));
            assert!(
                lut.data
                    .iter()
                    .all(|c| c.iter().all(|v| (0.0..=1.0).contains(v)))
            );
            assert_ne!(lut, Lut3::identity(17), "{name} changes colours");
        }
        assert!(builtin("nope").is_none());
    }

    #[test]
    fn a_missing_file_is_remembered_as_missing() {
        let mut cache = LutCache::default();
        let src = LutSource::File("/no/such/file.cube".into());
        assert!(cache.get(&src).is_none());
        assert!(cache.get(&src).is_none());
        assert!(
            cache
                .get(&LutSource::Builtin("teal_orange".into()))
                .is_some()
        );
    }
}
