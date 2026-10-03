//! The transition library: each transition is a name plus the recipe the renderer follows.
//!
//! Everything here is original (procedural) work: the recipes only describe how to blend two pictures.

use std::sync::OnceLock;
use vz_core::{Micros, US_PER_SEC};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Basic,
    Movement,
    Wipes,
    Effects,
    ThreeD,
}

impl Group {
    pub const ALL: [Self; 5] = [
        Self::Basic,
        Self::Movement,
        Self::Wipes,
        Self::Effects,
        Self::ThreeD,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Basic => "Basic",
            Self::Movement => "Movement",
            Self::Wipes => "Wipes",
            Self::Effects => "Effects",
            Self::ThreeD => "3D",
        }
    }
}

/// The recipe for blending the outgoing picture A and the incoming picture B.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Recipe {
    /// A fades into B.
    Dissolve,
    /// A fades to a colour, then the colour fades to B.
    FadeThrough([u8; 3]),
    /// Both go soft in the middle while one dissolves into the other.
    BlurDissolve,
    /// B slides in over A, from the direction (dx, dy) (1, 0 means from the right).
    Slide {
        dx: f32,
        dy: f32,
    },
    /// B pushes A out of the picture, moving in direction (dx, dy).
    Push {
        dx: f32,
        dy: f32,
    },
    /// A straight edge sweeps over the picture at `angle` degrees (0 = from the left edge to the right).
    Wipe {
        angle: f32,
    },
    /// Like a clock hand turning around the middle.
    Clock,
    /// A circle opens in the middle (or closes from the edges) showing B.
    Iris {
        open: bool,
    },
    Diamond,
    /// B opens from the middle outwards, like two barn doors (vertical: across).
    BarnDoor {
        vertical: bool,
    },
    Blinds {
        count: u32,
        vertical: bool,
    },
    /// A splits in two halves that slide apart, uncovering B.
    Split {
        vertical: bool,
    },
    /// A grows towards the viewer and fades into B (`inward`), or B shrinks into place from large.
    Zoom {
        inward: bool,
    },
    Spin,
    /// A quick bright flash covers the cut.
    Flash,
    Pixelate,
    /// Colour channels drift apart and the picture tears sideways, then settles on B.
    Glitch,
    /// A card flips over (vertical: around the horizontal axis).
    Flip {
        vertical: bool,
    },
    /// A box turns: A and B are two of its faces.
    Cube {
        dir: f32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TransitionPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub group: Group,
    pub recipe: Recipe,
    pub default_duration: Micros,
}

fn p(
    id: &'static str,
    name: &'static str,
    group: Group,
    recipe: Recipe,
    seconds: f64,
) -> TransitionPreset {
    TransitionPreset {
        id,
        name,
        group,
        recipe,
        default_duration: (seconds * US_PER_SEC as f64) as Micros,
    }
}

fn build() -> Vec<TransitionPreset> {
    use Group::{Basic, Effects, Movement, ThreeD, Wipes};
    use Recipe::*;
    vec![
        p("dissolve", "Mix", Basic, Dissolve, 1.0),
        p(
            "fade_black",
            "Fade to black",
            Basic,
            FadeThrough([0, 0, 0]),
            1.0,
        ),
        p(
            "fade_white",
            "Fade to white",
            Basic,
            FadeThrough([255, 255, 255]),
            1.0,
        ),
        p("blur_dissolve", "Blur mix", Basic, BlurDissolve, 1.0),
        p(
            "slide_left",
            "Slide left",
            Movement,
            Slide { dx: 1.0, dy: 0.0 },
            0.6,
        ),
        p(
            "slide_right",
            "Slide right",
            Movement,
            Slide { dx: -1.0, dy: 0.0 },
            0.6,
        ),
        p(
            "slide_up",
            "Slide up",
            Movement,
            Slide { dx: 0.0, dy: 1.0 },
            0.6,
        ),
        p(
            "slide_down",
            "Slide down",
            Movement,
            Slide { dx: 0.0, dy: -1.0 },
            0.6,
        ),
        p(
            "push_left",
            "Push left",
            Movement,
            Push { dx: 1.0, dy: 0.0 },
            0.6,
        ),
        p(
            "push_right",
            "Push right",
            Movement,
            Push { dx: -1.0, dy: 0.0 },
            0.6,
        ),
        p(
            "push_up",
            "Push up",
            Movement,
            Push { dx: 0.0, dy: 1.0 },
            0.6,
        ),
        p(
            "push_down",
            "Push down",
            Movement,
            Push { dx: 0.0, dy: -1.0 },
            0.6,
        ),
        p("zoom_in", "Zoom in", Movement, Zoom { inward: true }, 0.8),
        p(
            "zoom_out",
            "Zoom out",
            Movement,
            Zoom { inward: false },
            0.8,
        ),
        p("spin", "Spin", Movement, Spin, 0.9),
        p("wipe_right", "Wipe right", Wipes, Wipe { angle: 0.0 }, 0.7),
        p("wipe_left", "Wipe left", Wipes, Wipe { angle: 180.0 }, 0.7),
        p("wipe_down", "Wipe down", Wipes, Wipe { angle: 90.0 }, 0.7),
        p("wipe_up", "Wipe up", Wipes, Wipe { angle: 270.0 }, 0.7),
        p(
            "wipe_diagonal",
            "Diagonal wipe",
            Wipes,
            Wipe { angle: 45.0 },
            0.8,
        ),
        p("clock", "Clock wipe", Wipes, Clock, 1.0),
        p("iris_open", "Iris open", Wipes, Iris { open: true }, 0.8),
        p("iris_close", "Iris close", Wipes, Iris { open: false }, 0.8),
        p("diamond", "Diamond", Wipes, Diamond, 0.9),
        p(
            "barn_door",
            "Barn door",
            Wipes,
            BarnDoor { vertical: false },
            0.8,
        ),
        p(
            "barn_door_v",
            "Barn door (across)",
            Wipes,
            BarnDoor { vertical: true },
            0.8,
        ),
        p(
            "blinds",
            "Blinds",
            Wipes,
            Blinds {
                count: 8,
                vertical: false,
            },
            0.9,
        ),
        p(
            "blinds_v",
            "Blinds (across)",
            Wipes,
            Blinds {
                count: 8,
                vertical: true,
            },
            0.9,
        ),
        p("split", "Split", Wipes, Split { vertical: false }, 0.8),
        p(
            "split_v",
            "Split (across)",
            Wipes,
            Split { vertical: true },
            0.8,
        ),
        p("flash", "Flash", Effects, Flash, 0.5),
        p("pixelate", "Pixelate", Effects, Pixelate, 0.9),
        p("glitch", "Glitch", Effects, Glitch, 0.7),
        p("flip", "Flip", ThreeD, Flip { vertical: false }, 0.9),
        p(
            "flip_v",
            "Flip (up-down)",
            ThreeD,
            Flip { vertical: true },
            0.9,
        ),
        p("cube_left", "Cube left", ThreeD, Cube { dir: 1.0 }, 0.9),
        p("cube_right", "Cube right", ThreeD, Cube { dir: -1.0 }, 0.9),
    ]
}

fn all() -> &'static [TransitionPreset] {
    static ALL: OnceLock<Vec<TransitionPreset>> = OnceLock::new();
    ALL.get_or_init(build)
}

/// Every transition, in the order the panel shows them.
pub fn transitions() -> &'static [TransitionPreset] {
    all()
}

pub fn find(id: &str) -> Option<&'static TransitionPreset> {
    all().iter().find(|t| t.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_library_has_more_than_fifteen_transitions_in_every_group_with_unique_ids() {
        assert!(transitions().len() >= 30);
        for g in Group::ALL {
            assert!(transitions().iter().any(|t| t.group == g), "{g:?}");
        }
        let mut ids: Vec<_> = transitions().iter().map(|t| t.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n);
        assert!(transitions().iter().all(|t| t.default_duration > 0));
    }

    #[test]
    fn a_transition_can_be_found_by_its_id() {
        assert_eq!(find("dissolve").unwrap().name, "Mix");
        assert!(find("nope").is_none());
    }
}
