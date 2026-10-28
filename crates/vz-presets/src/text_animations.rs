//! Animated text presets (the "Text animations" panel): a typewriter with a blinking cursor, neon signs, a mirror,
//! two cinematic titles, words that rise in, text that shows the video through its letters, and an outline pop.
//! Each is a group of clips built for the canvas it is added to, like the text templates.

use crate::templates::{Canvas, Template, animated, stroke};
use vz_anim::{Animatable, Dir, Easing, Family, Keyframe};
use vz_core::{
    BlendMode, Clip, Color, Micros, Reflection, TextAnimator, TextBackground, TextGlow, TextPhase,
    TextShadow, TextUnit, TrackKind, US_PER_SEC, WHITE,
};

const INK: Color = [10, 10, 10, 255];
const CYAN: Color = [90, 235, 255, 255];
const PINK: Color = [255, 70, 190, 255];
const LIME: Color = [150, 255, 60, 255];

fn secs(s: f32) -> Micros {
    (s * US_PER_SEC as f32) as Micros
}

/// A size that goes from `from` to `to` over the clip: a slow push-in.
fn push_in(clip: &mut Clip, from: f32, to: f32, length: Micros) {
    let keys = |a: f32, b: f32| {
        Animatable::Keyframed(vec![
            Keyframe {
                time: 0,
                value: a,
                easing: Easing::Linear,
            },
            Keyframe {
                time: length,
                value: b,
                easing: Easing::Linear,
            },
        ])
    };
    clip.transform.scale_x = keys(from, to);
    clip.transform.scale_y = keys(from, to);
}

/// Letters that appear one by one, with a blinking cursor after the last one.
fn typewriter(c: &Canvas) -> Vec<(TrackKind, Clip)> {
    let len = c.len;
    let clip = animated(
        c.at(
            c.text(
                "BANGKOK  •  10AM  •  DECEMBER 3RD",
                |t| {
                    t.style.size = 70.0;
                    t.style.font = "Courier New".into();
                    t.style.bold = true;
                    t.style.letter_spacing = 0.02;
                    t.style.shadow = Some(TextShadow {
                        color: [0, 0, 0, 170],
                        dx: 0.0,
                        dy: 3.0,
                        blur: 10.0,
                    });
                    t.caret = true;
                    t.animators = vec![TextAnimator {
                        dy: 0.0,
                        delay: 0.07,
                        duration: 0.01,
                        easing: Easing::Linear,
                        ..TextAnimator::default()
                    }];
                },
                0,
                len,
            ),
            0.0,
            0.3,
        ),
        None,
        Some(("fade_out", 0.4)),
        None,
    );
    vec![(TrackKind::Text, clip)]
}

/// A neon sign: the letters switch on one by one, then flicker now and then.
fn neon_sign(c: &Canvas, color: Color, core: Color) -> Vec<(TrackKind, Clip)> {
    let len = c.len;
    let clip = c.text(
        "NEON NIGHTS",
        |t| {
            t.style.size = 190.0;
            t.style.bold = true;
            t.style.italic = true;
            t.style.color = core;
            t.style.letter_spacing = 0.03;
            t.style.stroke = stroke(color, 3.0);
            t.style.glow = Some(TextGlow {
                color,
                radius: 60.0,
                strength: 1.0,
            });
            t.animators = vec![
                // switching on, letter by letter
                TextAnimator {
                    dy: 0.0,
                    opacity: 0.0,
                    delay: 0.16,
                    duration: 0.35,
                    easing: Easing::Linear,
                    ..TextAnimator::default()
                },
                // and the odd flicker, a different one for every letter
                TextAnimator {
                    phase: TextPhase::Loop,
                    flicker: true,
                    dy: 0.0,
                    opacity: 0.25,
                    delay: 0.0,
                    duration: 1.6,
                    ..TextAnimator::default()
                },
            ];
        },
        0,
        len,
    );
    vec![(
        TrackKind::Text,
        animated(c.at(clip, 0.0, 0.0), None, Some(("fade_out", 0.5)), None),
    )]
}

fn neon_white(c: &Canvas) -> Vec<(TrackKind, Clip)> {
    neon_sign(c, [255, 255, 255, 255], [255, 255, 255, 255])
}

fn neon_pink(c: &Canvas) -> Vec<(TrackKind, Clip)> {
    neon_sign(c, PINK, [255, 225, 245, 255])
}

fn neon_cyan(c: &Canvas) -> Vec<(TrackKind, Clip)> {
    neon_sign(c, CYAN, [235, 255, 255, 255])
}

/// The word with a fading reflection under it.
fn mirror(c: &Canvas) -> Vec<(TrackKind, Clip)> {
    let len = c.len;
    let clip = animated(
        c.at(
            c.text(
                "MIRROR",
                |t| {
                    t.style.size = 230.0;
                    t.style.bold = true;
                    t.reflection = Some(Reflection {
                        gap: 4.0,
                        length: 0.7,
                        opacity: 0.6,
                    });
                },
                0,
                len,
            ),
            0.0,
            -0.06,
        ),
        Some(("fade_in", 0.8)),
        Some(("fade_out", 0.6)),
        None,
    );
    vec![(TrackKind::Text, clip)]
}

/// Wide-spaced letters drift together while they fade in.
fn cinematic_spread(c: &Canvas) -> Vec<(TrackKind, Clip)> {
    let len = c.len;
    let mut clip = c.text(
        "CINEMATIC",
        |t| {
            t.style.size = 150.0;
            t.style.bold = true;
            t.style.letter_spacing = 0.18;
            t.animators = vec![TextAnimator {
                dy: 0.0,
                opacity: 0.0,
                spread: 34.0,
                delay: 0.0,
                duration: 2.2,
                easing: Easing::Named(Family::Cubic, Dir::Out),
                ..TextAnimator::default()
            }];
        },
        0,
        len,
    );
    push_in(&mut clip, 1.0, 1.08, len);
    vec![(
        TrackKind::Text,
        animated(c.at(clip, 0.0, 0.0), None, Some(("blur_out", 0.9)), None),
    )]
}

/// Soft, glowing lower-case text that comes out of a blur and goes back into one.
fn cinematic_blur(c: &Canvas) -> Vec<(TrackKind, Clip)> {
    let len = c.len;
    let mut clip = c.text(
        "cinematic",
        |t| {
            t.style.size = 200.0;
            t.style.bold = true;
            t.style.glow = Some(TextGlow {
                color: [255, 255, 255, 255],
                radius: 40.0,
                strength: 0.55,
            });
        },
        0,
        len,
    );
    push_in(&mut clip, 1.0, 1.12, len);
    vec![(
        TrackKind::Text,
        animated(
            c.at(clip, 0.0, 0.0),
            Some(("blur_in", 1.2)),
            Some(("blur_out", 1.0)),
            None,
        ),
    )]
}

/// Words come in one by one, rising and settling: a calm, clean title.
fn apple_words(c: &Canvas) -> Vec<(TrackKind, Clip)> {
    let len = c.len;
    let clip = c.text(
        "smooth apple inspired animations",
        |t| {
            t.style.size = 96.0;
            t.style.bold = true;
            t.animators = vec![
                TextAnimator {
                    unit: TextUnit::Word,
                    dy: 36.0,
                    scale: 0.92,
                    opacity: 0.0,
                    delay: 0.14,
                    duration: 0.7,
                    easing: Easing::Named(Family::Cubic, Dir::Out),
                    ..TextAnimator::default()
                },
                TextAnimator {
                    unit: TextUnit::Word,
                    phase: TextPhase::Out,
                    dy: -24.0,
                    opacity: 0.0,
                    delay: 0.06,
                    duration: 0.45,
                    easing: Easing::EaseIn,
                    ..TextAnimator::default()
                },
            ];
        },
        0,
        len,
    );
    vec![(TrackKind::Text, c.at(clip, 0.0, 0.0))]
}

/// Huge letters that let the video below show through them (the rest goes black). Put it above a video.
fn video_in_text(c: &Canvas) -> Vec<(TrackKind, Clip)> {
    let len = c.len;
    let mut clip = c.text(
        "NYC",
        |t| {
            t.style.size = 520.0;
            t.style.bold = true;
            t.style.color = WHITE;
            // a black box that covers the whole picture; the letters are white inside it
            t.style.background = Some(TextBackground {
                color: [0, 0, 0, 255],
                padding: 700.0,
                radius: 0.0,
            });
        },
        0,
        len,
    );
    clip.blend = BlendMode::Multiply;
    // slowly pushing in; never smaller than the picture, so the black always covers it
    push_in(&mut clip, 1.0, 1.35, len);
    vec![(TrackKind::Text, c.at(clip, 0.0, 0.0))]
}

/// Chunky text with a thick outline that pops in word by word.
fn outline_pop(c: &Canvas) -> Vec<(TrackKind, Clip)> {
    let len = c.len;
    let clip = c.text(
        "tomorrow begins here.",
        |t| {
            t.style.size = 120.0;
            t.style.bold = true;
            t.style.color = LIME;
            t.style.stroke = stroke(INK, 10.0);
            t.animators = vec![TextAnimator {
                unit: TextUnit::Word,
                dy: 0.0,
                scale: 0.0,
                delay: 0.25,
                duration: 0.45,
                easing: Easing::Named(Family::Back, Dir::Out),
                ..TextAnimator::default()
            }];
        },
        0,
        len,
    );
    vec![(
        TrackKind::Text,
        animated(c.at(clip, 0.0, 0.0), None, Some(("fade_out", 0.4)), None),
    )]
}

/// The animated text presets, in the order the panel shows them.
pub fn text_animations() -> Vec<Template> {
    let t = |id, name, seconds: f32, build| Template::new(id, name, secs(seconds), build);
    vec![
        t("typewriter", "Typewriter", 6.0, typewriter),
        t("neon_white", "Neon sign", 5.0, neon_white),
        t("neon_pink", "Neon pink", 5.0, neon_pink),
        t("neon_cyan", "Neon cyan", 5.0, neon_cyan),
        t("mirror", "Mirror", 4.0, mirror),
        t(
            "cinematic_spread",
            "Cinematic spread",
            5.0,
            cinematic_spread,
        ),
        t("cinematic_blur", "Cinematic blur", 4.0, cinematic_blur),
        t("apple_words", "Words rise", 5.0, apple_words),
        t("video_in_text", "Video in text", 6.0, video_in_text),
        t("outline_pop", "Outline pop", 4.0, outline_pop),
    ]
}
