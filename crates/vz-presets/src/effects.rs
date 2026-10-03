//! The effects library: each effect has a name, a recipe the renderer follows, and the controls it offers.

use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Basic,
    LightLens,
    Retro,
    Distortion,
    Party,
}

impl Group {
    pub const ALL: [Self; 5] = [
        Self::Basic,
        Self::LightLens,
        Self::Retro,
        Self::Distortion,
        Self::Party,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Basic => "Basic",
            Self::LightLens => "Light & lens",
            Self::Retro => "Retro",
            Self::Distortion => "Distortion",
            Self::Party => "Party",
        }
    }
}

/// What an effect does (the renderer has one routine for each).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Blur,
    ZoomBlur,
    RadialBlur,
    MotionBlur,
    Sharpen,
    Vignette,
    FilmGrain,
    Letterbox,
    Mirror,
    Flip,
    SplitScreen,
    Glow,
    Flash,
    Strobe,
    ChromaticAberration,
    Prism,
    LightLeak,
    LensFlare,
    Vhs,
    OldFilm,
    BlackWhite,
    Sepia,
    Crt,
    Pixelate,
    Halftone,
    Shake,
    Wave,
    Swirl,
    Fisheye,
    Bulge,
    Ripple,
    Glitch,
    RgbSplit,
    DigitalNoise,
    Kaleidoscope,
    Tile,
    PulseZoom,
    Flicker,
    ColorCycle,
}

/// One control of an effect: a slider from `min` to `max`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Param {
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EffectPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub group: Group,
    pub kind: Kind,
    pub params: Vec<Param>,
}

impl EffectPreset {
    /// The starting values of the controls.
    pub fn defaults(&self) -> Vec<f32> {
        self.params.iter().map(|p| p.default).collect()
    }
}

const fn p(name: &'static str, min: f32, max: f32, default: f32) -> Param {
    Param {
        name,
        min,
        max,
        default,
    }
}

const AMOUNT: Param = p("Intensity", 0.0, 100.0, 60.0);
const SPEED: Param = p("Speed", 0.0, 100.0, 50.0);

fn e(
    id: &'static str,
    name: &'static str,
    group: Group,
    kind: Kind,
    params: &[Param],
) -> EffectPreset {
    EffectPreset {
        id,
        name,
        group,
        kind,
        params: params.to_vec(),
    }
}

fn build() -> Vec<EffectPreset> {
    use Group::{Basic, LightLens, Party, Retro};
    use Kind::*;
    let dist = Group::Distortion;
    vec![
        // ---- Basic
        e("blur", "Blur", Basic, Blur, &[AMOUNT]),
        e("zoom_blur", "Zoom blur", Basic, ZoomBlur, &[AMOUNT]),
        e("radial_blur", "Spin blur", Basic, RadialBlur, &[AMOUNT]),
        e(
            "motion_blur",
            "Motion blur",
            Basic,
            MotionBlur,
            &[AMOUNT, p("Angle", 0.0, 360.0, 0.0)],
        ),
        e("sharpen", "Sharpen", Basic, Sharpen, &[AMOUNT]),
        e("vignette", "Vignette", Basic, Vignette, &[AMOUNT]),
        e("film_grain", "Film grain", Basic, FilmGrain, &[AMOUNT]),
        e(
            "letterbox",
            "Letterbox bars",
            Basic,
            Letterbox,
            &[p("Bar size", 0.0, 100.0, 40.0)],
        ),
        e(
            "mirror",
            "Mirror",
            Basic,
            Mirror,
            &[p("Direction", 0.0, 3.0, 0.0)],
        ),
        e(
            "flip",
            "Flip",
            Basic,
            Flip,
            &[p("Direction", 0.0, 2.0, 0.0)],
        ),
        e(
            "split_screen",
            "Split screen",
            Basic,
            SplitScreen,
            &[p("Parts (2, 3, 4, 9)", 2.0, 9.0, 4.0)],
        ),
        // ---- Light & lens
        e(
            "glow",
            "Glow",
            LightLens,
            Glow,
            &[AMOUNT, p("Spread", 0.0, 100.0, 50.0)],
        ),
        e("flash", "Flash", LightLens, Flash, &[AMOUNT, SPEED]),
        e("strobe", "Strobe", LightLens, Strobe, &[AMOUNT, SPEED]),
        e(
            "chromatic",
            "Chromatic aberration",
            LightLens,
            ChromaticAberration,
            &[AMOUNT],
        ),
        e("prism", "Prism", LightLens, Prism, &[AMOUNT]),
        e(
            "light_leak",
            "Light leak",
            LightLens,
            LightLeak,
            &[AMOUNT, SPEED],
        ),
        e(
            "lens_flare",
            "Lens flare",
            LightLens,
            LensFlare,
            &[AMOUNT, SPEED],
        ),
        // ---- Retro
        e("vhs", "VHS", Retro, Vhs, &[AMOUNT, SPEED]),
        e("old_film", "Old film", Retro, OldFilm, &[AMOUNT]),
        e("black_white", "Black & white", Retro, BlackWhite, &[AMOUNT]),
        e("sepia", "Sepia", Retro, Sepia, &[AMOUNT]),
        e("crt", "CRT scanlines", Retro, Crt, &[AMOUNT]),
        e(
            "pixelate",
            "8-bit pixels",
            Retro,
            Pixelate,
            &[p("Block size", 1.0, 100.0, 30.0)],
        ),
        e(
            "halftone",
            "Halftone",
            Retro,
            Halftone,
            &[AMOUNT, p("Dot size", 1.0, 100.0, 30.0)],
        ),
        // ---- Distortion
        e("shake", "Camera shake", dist, Shake, &[AMOUNT, SPEED]),
        e("wave", "Wave", dist, Wave, &[AMOUNT, SPEED]),
        e("swirl", "Swirl", dist, Swirl, &[AMOUNT]),
        e("fisheye", "Fisheye", dist, Fisheye, &[AMOUNT]),
        e(
            "bulge",
            "Bulge / pinch",
            dist,
            Bulge,
            &[p("Amount", -100.0, 100.0, 60.0)],
        ),
        e("ripple", "Ripple", dist, Ripple, &[AMOUNT, SPEED]),
        e("glitch", "Glitch", dist, Glitch, &[AMOUNT, SPEED]),
        e(
            "rgb_split",
            "RGB split",
            dist,
            RgbSplit,
            &[AMOUNT, p("Angle", 0.0, 360.0, 0.0)],
        ),
        e(
            "digital_noise",
            "Digital noise",
            dist,
            DigitalNoise,
            &[AMOUNT],
        ),
        e(
            "kaleidoscope",
            "Kaleidoscope",
            dist,
            Kaleidoscope,
            &[p("Segments", 2.0, 16.0, 6.0), SPEED],
        ),
        e("tile", "Tile", dist, Tile, &[p("Tiles", 1.0, 8.0, 3.0)]),
        // ---- Party
        e(
            "pulse_zoom",
            "Pulse zoom",
            Party,
            PulseZoom,
            &[AMOUNT, SPEED],
        ),
        e("flicker", "Flicker", Party, Flicker, &[AMOUNT, SPEED]),
        e(
            "color_cycle",
            "Colour cycle",
            Party,
            ColorCycle,
            &[AMOUNT, SPEED],
        ),
    ]
}

fn all() -> &'static [EffectPreset] {
    static ALL: OnceLock<Vec<EffectPreset>> = OnceLock::new();
    ALL.get_or_init(build)
}

pub fn effects() -> &'static [EffectPreset] {
    all()
}

pub fn find(id: &str) -> Option<&'static EffectPreset> {
    all().iter().find(|x| x.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_more_than_25_effects_in_every_group_with_unique_ids_and_sane_controls() {
        assert!(effects().len() >= 30);
        for g in Group::ALL {
            assert!(effects().iter().any(|x| x.group == g), "{g:?}");
        }
        let mut ids: Vec<_> = effects().iter().map(|x| x.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n);
        for x in effects() {
            assert!(!x.params.is_empty() && x.params.len() <= 3, "{}", x.id);
            for p in &x.params {
                assert!(
                    p.min < p.max && (p.min..=p.max).contains(&p.default),
                    "{} {}",
                    x.id,
                    p.name
                );
            }
        }
    }

    #[test]
    fn defaults_follow_the_list_of_controls() {
        let blur = find("blur").unwrap();
        assert_eq!(blur.defaults(), vec![60.0]);
        assert!(find("nope").is_none());
    }
}
