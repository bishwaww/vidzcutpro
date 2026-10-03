//! Caption styles: ready-made looks for captions, including the word-by-word ones (karaoke, reveal, one word at a time,
//! keyword). Each is plain data, so they can be changed or added to without touching the program.

use std::sync::OnceLock;
use vz_anim::Easing;
use vz_core::{
    BLACK, CaptionLook, Color, Gradient, TextAnimator, TextBackground, TextGlow, TextPhase,
    TextShadow, TextStroke, TextStyle, TextUnit, WHITE, WordMode,
};

#[derive(Clone, Debug, PartialEq)]
pub struct CaptionPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub look: CaptionLook,
    /// Cut captions to this many words each when the style is applied (for the quick word-by-word looks).
    pub words_per_caption: Option<usize>,
    /// Make the letters capitals.
    pub uppercase: bool,
}

const YELLOW: Color = [255, 214, 10, 255];
const GREEN: Color = [60, 255, 107, 255];
const CYAN: Color = [0, 229, 255, 255];
const RED: Color = [255, 64, 64, 255];

fn base(f: impl FnOnce(&mut TextStyle)) -> TextStyle {
    let mut s = TextStyle {
        size: 64.0,
        bold: true,
        ..TextStyle::default()
    };
    f(&mut s);
    s
}

fn stroke(width: f32) -> Option<TextStroke> {
    Some(TextStroke {
        color: BLACK,
        width,
    })
}

fn look(style: TextStyle, words: WordMode, animators: Vec<TextAnimator>) -> CaptionLook {
    CaptionLook {
        style,
        words,
        animators,
        y: 0.3,
    }
}

fn fade_in_out() -> Vec<TextAnimator> {
    let mk = |phase| TextAnimator {
        unit: TextUnit::Line,
        phase,
        dx: 0.0,
        dy: 0.0,
        scale: 1.0,
        rotation: 0.0,
        opacity: 0.0,
        delay: 0.0,
        duration: 0.25,
        easing: Easing::Linear,
    };
    vec![mk(TextPhase::In), mk(TextPhase::Out)]
}

fn p(
    id: &'static str,
    name: &'static str,
    look: CaptionLook,
    words: Option<usize>,
    uppercase: bool,
) -> CaptionPreset {
    CaptionPreset {
        id,
        name,
        look,
        words_per_caption: words,
        uppercase,
    }
}

fn build() -> Vec<CaptionPreset> {
    let karaoke = |c: Color, scale: f32| WordMode::Karaoke {
        highlight: c,
        scale,
    };
    vec![
        p(
            "karaoke_yellow",
            "Karaoke Yellow",
            look(
                base(|s| s.stroke = stroke(7.0)),
                karaoke(YELLOW, 1.0),
                vec![],
            ),
            None,
            false,
        ),
        p(
            "green_pop",
            "Green Pop",
            look(
                base(|s| s.stroke = stroke(8.0)),
                karaoke(GREEN, 1.15),
                vec![],
            ),
            Some(4),
            false,
        ),
        p(
            "reels_3",
            "Reels 3 Words",
            look(base(|s| s.stroke = stroke(8.0)), karaoke(CYAN, 1.0), vec![]),
            Some(3),
            true,
        ),
        p(
            "one_word_pop",
            "One Word Pop",
            look(
                base(|s| {
                    s.size = 86.0;
                    s.stroke = stroke(9.0);
                }),
                WordMode::Single { scale: 1.2 },
                vec![TextAnimator {
                    unit: TextUnit::Line,
                    scale: 0.6,
                    opacity: 0.0,
                    dx: 0.0,
                    dy: 0.0,
                    delay: 0.0,
                    duration: 0.15,
                    easing: Easing::EaseOut,
                    ..TextAnimator::default()
                }],
            ),
            Some(1),
            false,
        ),
        p(
            "word_reveal",
            "Word Reveal",
            look(
                base(|s| s.stroke = stroke(7.0)),
                WordMode::Reveal {
                    highlight: Some(YELLOW),
                },
                vec![],
            ),
            None,
            false,
        ),
        p(
            "red_keyword",
            "Red Keyword",
            look(
                base(|s| s.stroke = stroke(7.0)),
                WordMode::Keywords {
                    highlight: RED,
                    scale: 1.2,
                    words: vec![],
                },
                vec![],
            ),
            None,
            false,
        ),
        p(
            "classic_white",
            "Classic White",
            look(
                base(|s| {
                    s.bold = false;
                    s.stroke = stroke(4.0);
                    s.shadow = Some(TextShadow {
                        color: [0, 0, 0, 170],
                        blur: 6.0,
                        dx: 2.0,
                        dy: 3.0,
                    });
                }),
                WordMode::Off,
                vec![],
            ),
            None,
            false,
        ),
        p(
            "soft_shadow",
            "Soft Shadow",
            look(
                base(|s| {
                    s.shadow = Some(TextShadow {
                        color: [0, 0, 0, 200],
                        blur: 14.0,
                        dx: 0.0,
                        dy: 4.0,
                    })
                }),
                WordMode::Off,
                vec![],
            ),
            None,
            false,
        ),
        p(
            "sunny_yellow",
            "Sunny Yellow",
            look(
                base(|s| {
                    s.color = YELLOW;
                    s.stroke = stroke(6.0);
                }),
                WordMode::Off,
                vec![],
            ),
            None,
            false,
        ),
        p(
            "bold_outline",
            "Bold Outline",
            look(base(|s| s.stroke = stroke(11.0)), WordMode::Off, vec![]),
            None,
            false,
        ),
        p(
            "gold_fade",
            "Gold Fade",
            look(
                base(|s| {
                    s.gradient = Some(Gradient::two(
                        [255, 232, 130, 255],
                        [214, 140, 20, 255],
                        90.0,
                    ));
                    s.stroke = stroke(5.0);
                }),
                WordMode::Off,
                fade_in_out(),
            ),
            None,
            false,
        ),
        p(
            "black_box",
            "Black Box",
            look(
                base(|s| {
                    s.background = Some(TextBackground {
                        color: [0, 0, 0, 190],
                        padding: 18.0,
                        radius: 10.0,
                    })
                }),
                WordMode::Off,
                vec![],
            ),
            None,
            false,
        ),
        p(
            "yellow_box",
            "Yellow Box",
            look(
                base(|s| {
                    s.color = [10, 10, 10, 255];
                    s.background = Some(TextBackground {
                        color: YELLOW,
                        padding: 18.0,
                        radius: 10.0,
                    });
                }),
                WordMode::Off,
                vec![],
            ),
            None,
            false,
        ),
        p(
            "white_box",
            "White Box",
            look(
                base(|s| {
                    s.color = [10, 10, 10, 255];
                    s.background = Some(TextBackground {
                        color: WHITE,
                        padding: 18.0,
                        radius: 10.0,
                    });
                }),
                WordMode::Off,
                vec![],
            ),
            None,
            false,
        ),
        p(
            "neon_blue",
            "Neon Blue",
            look(
                base(|s| {
                    s.color = [220, 250, 255, 255];
                    s.glow = Some(TextGlow {
                        color: CYAN,
                        radius: 26.0,
                        strength: 1.0,
                    });
                }),
                WordMode::Off,
                vec![],
            ),
            None,
            false,
        ),
        p(
            "neon_pink",
            "Neon Pink",
            look(
                base(|s| {
                    s.color = [255, 225, 245, 255];
                    s.glow = Some(TextGlow {
                        color: [255, 60, 190, 255],
                        radius: 26.0,
                        strength: 1.0,
                    });
                }),
                WordMode::Off,
                vec![],
            ),
            None,
            false,
        ),
        p(
            "warm_glow",
            "Warm Glow",
            look(
                base(|s| {
                    s.color = [255, 240, 210, 255];
                    s.glow = Some(TextGlow {
                        color: [255, 150, 40, 255],
                        radius: 22.0,
                        strength: 0.9,
                    });
                }),
                WordMode::Off,
                vec![],
            ),
            None,
            false,
        ),
        p(
            "fade_in_out",
            "Fade In & Out",
            look(
                base(|s| s.stroke = stroke(6.0)),
                WordMode::Off,
                fade_in_out(),
            ),
            None,
            false,
        ),
        p(
            "slide_up",
            "Slide Up",
            look(
                base(|s| s.stroke = stroke(6.0)),
                WordMode::Off,
                vec![TextAnimator {
                    unit: TextUnit::Line,
                    dy: 60.0,
                    dx: 0.0,
                    opacity: 0.0,
                    delay: 0.0,
                    duration: 0.35,
                    ..TextAnimator::default()
                }],
            ),
            None,
            false,
        ),
        p(
            "pop_up",
            "Pop Up",
            look(
                base(|s| s.stroke = stroke(6.0)),
                WordMode::Off,
                vec![TextAnimator {
                    unit: TextUnit::Word,
                    scale: 0.0,
                    opacity: 0.0,
                    dx: 0.0,
                    dy: 0.0,
                    delay: 0.08,
                    duration: 0.25,
                    easing: Easing::Named(vz_anim::Family::Back, vz_anim::Dir::Out),
                    ..TextAnimator::default()
                }],
            ),
            None,
            false,
        ),
        p(
            "typewriter",
            "Typewriter",
            look(
                base(|s| {
                    s.bold = false;
                    s.background = Some(TextBackground {
                        color: [0, 0, 0, 150],
                        padding: 14.0,
                        radius: 6.0,
                    });
                }),
                WordMode::Off,
                vec![TextAnimator {
                    unit: TextUnit::Character,
                    dx: 0.0,
                    dy: 0.0,
                    opacity: 0.0,
                    delay: 0.04,
                    duration: 0.01,
                    easing: Easing::Linear,
                    ..TextAnimator::default()
                }],
            ),
            None,
            false,
        ),
    ]
}

fn all() -> &'static [CaptionPreset] {
    static ALL: OnceLock<Vec<CaptionPreset>> = OnceLock::new();
    ALL.get_or_init(build)
}

pub fn caption_presets() -> &'static [CaptionPreset] {
    all()
}

pub fn find(id: &str) -> Option<&'static CaptionPreset> {
    all().iter().find(|c| c.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prototype_looks_are_all_there_with_unique_ids() {
        for id in [
            "karaoke_yellow",
            "green_pop",
            "reels_3",
            "one_word_pop",
            "word_reveal",
            "red_keyword",
        ] {
            assert!(find(id).is_some(), "{id}");
        }
        assert!(caption_presets().len() >= 20);
        let mut ids: Vec<_> = caption_presets().iter().map(|c| c.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }

    #[test]
    fn word_by_word_looks_have_a_word_mode_and_the_quick_ones_cut_captions_short() {
        assert!(matches!(
            find("karaoke_yellow").unwrap().look.words,
            WordMode::Karaoke { .. }
        ));
        assert_eq!(find("reels_3").unwrap().words_per_caption, Some(3));
        assert!(find("reels_3").unwrap().uppercase);
        assert!(matches!(
            find("one_word_pop").unwrap().look.words,
            WordMode::Single { .. }
        ));
        assert_eq!(find("one_word_pop").unwrap().words_per_caption, Some(1));
        assert!(matches!(
            find("red_keyword").unwrap().look.words,
            WordMode::Keywords { .. }
        ));
        assert!(
            find("typewriter")
                .unwrap()
                .look
                .animators
                .iter()
                .any(|a| a.unit == TextUnit::Character)
        );
    }
}
