//! The filters library: ready-made colour looks. Each is a set of colour adjustments (and sometimes one of our own
//! looks from `vz-render`), so a filter can be fine-tuned afterwards on the Adjust tab.

use std::sync::OnceLock;
use vz_core::{Adjust, LutRef, LutSource, Wheel};

#[derive(Clone, Debug, PartialEq)]
pub struct FilterPreset {
    pub id: &'static str,
    pub name: &'static str,
    /// Index into [`GROUPS`].
    pub group: usize,
    pub adjust: Adjust,
}

/// The categories, in the order of the panel's list.
pub const GROUPS: [&str; 9] = [
    "Featured",
    "Life",
    "Landscape",
    "Portrait",
    "Movies",
    "Retro",
    "Food",
    "B&W",
    "Night",
];

fn look(name: &str, strength: f32) -> Option<LutRef> {
    Some(LutRef {
        source: LutSource::Builtin(name.to_owned()),
        strength,
    })
}

fn f(
    id: &'static str,
    name: &'static str,
    group: usize,
    set: impl FnOnce(&mut Adjust),
) -> FilterPreset {
    let mut adjust = Adjust::default();
    set(&mut adjust);
    FilterPreset {
        id,
        name,
        group,
        adjust,
    }
}

fn build() -> Vec<FilterPreset> {
    vec![
        // ---- Featured
        f("vivid", "Vivid", 0, |a| {
            a.saturation = 35.0;
            a.contrast = 15.0;
            a.vibrance = 20.0;
        }),
        f("clean", "Clean", 0, |a| {
            a.brightness = 6.0;
            a.contrast = 8.0;
            a.sharpen = 25.0;
            a.temperature = -4.0;
        }),
        f("cinema", "Cinema", 0, |a| {
            a.lut = look("teal_orange", 0.85);
            a.contrast = 12.0;
            a.vignette = 25.0;
        }),
        f("glow_up", "Glow up", 0, |a| {
            a.brightness = 8.0;
            a.saturation = 12.0;
            a.highlights = 15.0;
            a.fade = 8.0;
            a.temperature = 8.0;
        }),
        // ---- Life
        f("daylight", "Daylight", 1, |a| {
            a.temperature = 12.0;
            a.exposure = 8.0;
            a.saturation = 14.0;
        }),
        f("cozy", "Cozy", 1, |a| {
            a.temperature = 28.0;
            a.tint = 6.0;
            a.fade = 10.0;
            a.saturation = 8.0;
        }),
        f("fresh", "Fresh", 1, |a| {
            a.temperature = -10.0;
            a.saturation = 18.0;
            a.brightness = 8.0;
            a.vibrance = 15.0;
        }),
        f("soft", "Soft", 1, |a| {
            a.contrast = -12.0;
            a.brightness = 8.0;
            a.fade = 14.0;
            a.clarity = -20.0;
        }),
        // ---- Landscape
        f("deep_green", "Deep green", 2, |a| {
            a.hsl[3].saturation = 45.0;
            a.hsl[3].lightness = -15.0;
            a.hsl[2].hue = -15.0;
            a.contrast = 14.0;
        }),
        f("blue_sky", "Blue sky", 2, |a| {
            a.hsl[5].saturation = 55.0;
            a.hsl[4].saturation = 30.0;
            a.hsl[5].lightness = -12.0;
            a.clarity = 25.0;
        }),
        f("golden_hour", "Golden hour", 2, |a| {
            a.temperature = 35.0;
            a.tint = 5.0;
            a.highlights = 10.0;
            a.saturation = 15.0;
            a.vignette = 18.0;
        }),
        f("misty", "Misty", 2, |a| {
            a.fade = 28.0;
            a.contrast = -18.0;
            a.temperature = -8.0;
            a.brightness = 6.0;
        }),
        // ---- Portrait
        f("skin_glow", "Skin glow", 3, |a| {
            a.brightness = 8.0;
            a.hsl[0].saturation = -10.0;
            a.hsl[1].lightness = 12.0;
            a.clarity = -18.0;
            a.temperature = 6.0;
        }),
        f("warm_portrait", "Warm portrait", 3, |a| {
            a.temperature = 18.0;
            a.contrast = 8.0;
            a.wheels.gain = Wheel {
                red: 0.12,
                green: 0.02,
                blue: -0.1,
                luma: 0.0,
            };
        }),
        f("matte_skin", "Matte skin", 3, |a| {
            a.fade = 18.0;
            a.contrast = -8.0;
            a.saturation = -8.0;
            a.shadows = 12.0;
        }),
        f("studio", "Studio", 3, |a| {
            a.contrast = 14.0;
            a.sharpen = 30.0;
            a.highlights = -10.0;
            a.vignette = 15.0;
        }),
        // ---- Movies
        f("teal_orange", "Teal and orange", 4, |a| {
            a.lut = look("teal_orange", 1.0);
            a.contrast = 10.0;
        }),
        f("bleach", "Bleach bypass", 4, |a| {
            a.lut = look("bleach_bypass", 1.0);
            a.vignette = 20.0;
        }),
        f("noir_blue", "Cold thriller", 4, |a| {
            a.temperature = -25.0;
            a.saturation = -22.0;
            a.contrast = 22.0;
            a.shadows = -10.0;
            a.vignette = 30.0;
        }),
        f("blockbuster", "Blockbuster", 4, |a| {
            a.lut = look("teal_orange", 0.7);
            a.saturation = 10.0;
            a.contrast = 18.0;
            a.clarity = 15.0;
            a.vignette = 22.0;
        }),
        // ---- Retro
        f("faded_film", "Faded film", 5, |a| {
            a.lut = look("faded_film", 1.0);
            a.grain = 25.0;
        }),
        f("cross_process", "Cross process", 5, |a| {
            a.lut = look("cross_process", 1.0);
            a.contrast = 8.0;
        }),
        f("polaroid", "Polaroid", 5, |a| {
            a.fade = 20.0;
            a.temperature = 14.0;
            a.tint = -6.0;
            a.saturation = -8.0;
            a.vignette = 28.0;
            a.grain = 15.0;
        }),
        f("seventies", "Seventies", 5, |a| {
            a.temperature = 30.0;
            a.saturation = -12.0;
            a.fade = 24.0;
            a.grain = 20.0;
            a.hsl[1].saturation = 25.0;
        }),
        // ---- Food
        f("tasty", "Tasty", 6, |a| {
            a.saturation = 28.0;
            a.temperature = 10.0;
            a.contrast = 12.0;
            a.hsl[1].saturation = 30.0;
            a.sharpen = 25.0;
        }),
        f("fresh_salad", "Fresh salad", 6, |a| {
            a.hsl[3].saturation = 35.0;
            a.hsl[2].saturation = 20.0;
            a.brightness = 8.0;
            a.vibrance = 18.0;
        }),
        f("bakery", "Bakery", 6, |a| {
            a.temperature = 22.0;
            a.fade = 8.0;
            a.hsl[1].lightness = 8.0;
            a.saturation = 10.0;
        }),
        f("cafe", "Café", 6, |a| {
            a.temperature = 16.0;
            a.contrast = 10.0;
            a.fade = 12.0;
            a.vignette = 20.0;
        }),
        // ---- B&W
        f("mono", "Mono", 7, |a| {
            a.saturation = -100.0;
        }),
        f("high_contrast_bw", "Punchy mono", 7, |a| {
            a.saturation = -100.0;
            a.contrast = 40.0;
            a.clarity = 20.0;
            a.blacks = -10.0;
        }),
        f("soft_bw", "Soft mono", 7, |a| {
            a.saturation = -100.0;
            a.fade = 18.0;
            a.contrast = -6.0;
        }),
        f("silver", "Silver", 7, |a| {
            a.saturation = -100.0;
            a.temperature = -8.0;
            a.grain = 22.0;
            a.vignette = 25.0;
        }),
        // ---- Night
        f("neon_night", "Neon night", 8, |a| {
            a.saturation = 30.0;
            a.contrast = 20.0;
            a.shadows = -15.0;
            a.wheels.lift = Wheel {
                red: 0.05,
                green: -0.05,
                blue: 0.25,
                luma: 0.0,
            };
        }),
        f("moonlight", "Moonlight", 8, |a| {
            a.temperature = -30.0;
            a.exposure = -12.0;
            a.saturation = -15.0;
            a.contrast = 14.0;
        }),
        f("city_lights", "City lights", 8, |a| {
            a.contrast = 16.0;
            a.highlights = 14.0;
            a.shadows = 12.0;
            a.saturation = 14.0;
            a.sharpen = 20.0;
        }),
        f("dusk", "Dusk", 8, |a| {
            a.temperature = 12.0;
            a.tint = 12.0;
            a.exposure = -10.0;
            a.vignette = 30.0;
            a.fade = 8.0;
        }),
    ]
}

fn all() -> &'static [FilterPreset] {
    static ALL: OnceLock<Vec<FilterPreset>> = OnceLock::new();
    ALL.get_or_init(build)
}

pub fn filters() -> &'static [FilterPreset] {
    all()
}

pub fn find(id: &str) -> Option<&'static FilterPreset> {
    all().iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_group_has_filters_with_unique_ids_that_change_colours() {
        assert!(filters().len() >= 30);
        for (g, name) in GROUPS.iter().enumerate() {
            assert!(
                filters().iter().filter(|p| p.group == g).count() >= 4,
                "{name}"
            );
        }
        let mut ids: Vec<_> = filters().iter().map(|p| p.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n);
        assert!(filters().iter().all(|p| !p.adjust.is_identity()));
    }

    #[test]
    fn looks_named_by_filters_exist_in_the_renderer_list() {
        for p in filters() {
            if let Some(LutRef {
                source: LutSource::Builtin(name),
                ..
            }) = &p.adjust.lut
            {
                assert!(
                    [
                        "teal_orange",
                        "faded_film",
                        "bleach_bypass",
                        "cross_process"
                    ]
                    .contains(&name.as_str()),
                    "{name}"
                );
            }
        }
    }
}
