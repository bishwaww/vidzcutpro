//! Text templates: ready-made groups of text and shapes with their animations (a lower third, a quote card, a
//! countdown...). A template is built for the canvas it is added to, so it fits wide and tall pictures alike.

use vz_core::{
    AnimRef, Clip, Color, Micros, ShapeContent, ShapeKind, TextAlign, TextBackground, TextContent,
    TextStroke, TrackKind, US_PER_SEC, WHITE,
};

const YELLOW: Color = [252, 198, 16, 255];
const INK: Color = [10, 10, 10, 255];
const RED: Color = [214, 40, 40, 255];
const NAVY: Color = [16, 28, 64, 255];

/// A ready-made group of layers.
#[derive(Clone, Copy, Debug)]
pub struct Template {
    pub id: &'static str,
    pub name: &'static str,
    /// How long it stays on screen.
    pub duration: Micros,
    build: fn(&Canvas) -> Vec<(TrackKind, Clip)>,
}

impl Template {
    /// The layers of this template for a canvas of `width` by `height` pixels (bottom layer first; starts are relative
    /// to where the template is placed).
    pub fn build(&self, width: u32, height: u32) -> Vec<(TrackKind, Clip)> {
        (self.build)(&Canvas {
            w: width as f32,
            h: height as f32,
            len: self.duration,
        })
    }
}

/// The canvas a template is built for, with helpers to place things by share of its size.
struct Canvas {
    w: f32,
    h: f32,
    len: Micros,
}

impl Canvas {
    /// Put a clip `fx` and `fy` of the canvas width and height away from the middle.
    fn at(&self, mut clip: Clip, fx: f32, fy: f32) -> Clip {
        clip.transform.x = (fx * self.w).into();
        clip.transform.y = (fy * self.h).into();
        clip
    }

    fn text(
        &self,
        text: &str,
        f: impl FnOnce(&mut TextContent),
        start: Micros,
        duration: Micros,
    ) -> Clip {
        let mut t = TextContent::new(text);
        t.style.color = WHITE;
        f(&mut t);
        let mut c = Clip::text_clip(t, 0, duration);
        c.start = start;
        c
    }

    fn shape(
        &self,
        kind: ShapeKind,
        w: f32,
        h: f32,
        fill: Color,
        start: Micros,
        duration: Micros,
    ) -> Clip {
        let shape = ShapeContent {
            width: w,
            height: h,
            fill: Some(fill),
            ..ShapeContent::new(kind)
        };
        let mut c = Clip::shape_clip(shape, 0, duration);
        c.start = start;
        c
    }

    /// A width in reference pixels (the short side is 1080) that covers `share` of the canvas width.
    fn across(&self, share: f32) -> f32 {
        share * self.w * 1080.0 / self.w.min(self.h)
    }

    fn rounded(&self, w: f32, h: f32, radius: f32, fill: Color, duration: Micros) -> Clip {
        let shape = ShapeContent {
            width: w,
            height: h,
            fill: Some(fill),
            corner_radius: radius,
            ..ShapeContent::new(ShapeKind::Rectangle)
        };
        Clip::shape_clip(shape, 0, duration)
    }
}

fn anim(preset: &str, seconds: f32) -> Option<AnimRef> {
    Some(AnimRef {
        preset: preset.to_owned(),
        duration: (seconds * US_PER_SEC as f32) as Micros,
    })
}

/// Give a clip its animations: in, out and one that plays all the time.
fn animated(
    mut c: Clip,
    i: Option<(&str, f32)>,
    o: Option<(&str, f32)>,
    combo: Option<(&str, f32)>,
) -> Clip {
    c.anim_in = i.and_then(|(p, s)| anim(p, s));
    c.anim_out = o.and_then(|(p, s)| anim(p, s));
    c.anim_combo = combo.and_then(|(p, s)| anim(p, s));
    c
}

fn stroke(color: Color, width: f32) -> Option<TextStroke> {
    Some(TextStroke { color, width })
}

fn lower_third(c: &Canvas) -> Vec<(TrackKind, Clip)> {
    let len = c.len;
    let bar = animated(
        c.at(
            c.shape(ShapeKind::Rectangle, 14.0, 150.0, YELLOW, 0, len),
            -0.36,
            0.3,
        ),
        Some(("wipe_in", 0.5)),
        Some(("fade_out", 0.4)),
        None,
    );
    let name = animated(
        c.at(
            c.text(
                "Your name",
                |t| {
                    t.style.size = 76.0;
                    t.style.bold = true;
                    t.style.align = TextAlign::Left;
                },
                300_000,
                len - 300_000,
            ),
            -0.2,
            0.275,
        ),
        Some(("slide_left_in", 0.6)),
        Some(("fade_out", 0.4)),
        None,
    );
    let role = animated(
        c.at(
            c.text(
                "Title or place",
                |t| {
                    t.style.size = 44.0;
                    t.style.color = YELLOW;
                    t.style.align = TextAlign::Left;
                },
                450_000,
                len - 450_000,
            ),
            -0.2,
            0.33,
        ),
        Some(("slide_left_in", 0.6)),
        Some(("fade_out", 0.4)),
        None,
    );
    vec![
        (TrackKind::Overlay, bar),
        (TrackKind::Text, name),
        (TrackKind::Text, role),
    ]
}

fn subscribe(c: &Canvas) -> Vec<(TrackKind, Clip)> {
    let len = c.len;
    let pill = animated(
        c.at(c.rounded(620.0, 150.0, 75.0, RED, len), 0.0, 0.34),
        Some(("pop_in", 0.5)),
        Some(("pop_out", 0.4)),
        Some(("pulse", 1.2)),
    );
    let label = animated(
        c.at(
            c.text(
                "SUBSCRIBE",
                |t| {
                    t.style.size = 72.0;
                    t.style.bold = true;
                },
                0,
                len,
            ),
            0.0,
            0.34,
        ),
        Some(("pop_in", 0.5)),
        Some(("pop_out", 0.4)),
        Some(("pulse", 1.2)),
    );
    vec![(TrackKind::Overlay, pill), (TrackKind::Text, label)]
}

fn quote_card(c: &Canvas) -> Vec<(TrackKind, Clip)> {
    let len = c.len;
    let card = animated(
        c.rounded(1100.0, 560.0, 36.0, [24, 24, 24, 235], len),
        Some(("fade_in", 0.6)),
        Some(("fade_out", 0.5)),
        None,
    );
    let mark = animated(
        c.at(
            c.text(
                "\u{201C}",
                |t| {
                    t.style.size = 260.0;
                    t.style.bold = true;
                    t.style.color = YELLOW;
                },
                0,
                len,
            ),
            0.0,
            -0.17,
        ),
        Some(("zoom_in", 0.7)),
        Some(("fade_out", 0.5)),
        None,
    );
    let quote = animated(
        c.at(
            c.text(
                "Write a quote here that is worth reading.",
                |t| {
                    t.style.size = 62.0;
                    t.style.box_width = Some(900.0);
                    t.style.align = TextAlign::Center;
                },
                400_000,
                len - 400_000,
            ),
            0.0,
            0.0,
        ),
        Some(("fade_in", 0.8)),
        Some(("fade_out", 0.5)),
        None,
    );
    let who = animated(
        c.at(
            c.text(
                "Someone wise",
                |t| {
                    t.style.size = 40.0;
                    t.style.color = YELLOW;
                },
                800_000,
                len - 800_000,
            ),
            0.0,
            0.17,
        ),
        Some(("fade_in", 0.8)),
        Some(("fade_out", 0.5)),
        None,
    );
    vec![
        (TrackKind::Overlay, card),
        (TrackKind::Text, mark),
        (TrackKind::Text, quote),
        (TrackKind::Text, who),
    ]
}

fn breaking_news(c: &Canvas) -> Vec<(TrackKind, Clip)> {
    let len = c.len;
    let bar = animated(
        c.at(c.rounded(c.across(0.96), 120.0, 0.0, NAVY, len), 0.0, 0.36),
        Some(("slide_right_in", 0.5)),
        Some(("slide_right_out", 0.4)),
        None,
    );
    let tag = animated(
        c.at(c.rounded(300.0, 120.0, 0.0, RED, len), -0.3, 0.36),
        Some(("slide_right_in", 0.5)),
        Some(("slide_right_out", 0.4)),
        None,
    );
    let tag_text = animated(
        c.at(
            c.text(
                "BREAKING",
                |t| {
                    t.style.size = 50.0;
                    t.style.bold = true;
                },
                100_000,
                len - 100_000,
            ),
            -0.3,
            0.36,
        ),
        Some(("fade_in", 0.5)),
        Some(("fade_out", 0.3)),
        None,
    );
    let headline = animated(
        c.at(
            c.text(
                "The headline goes here",
                |t| {
                    t.style.size = 48.0;
                    t.style.align = TextAlign::Left;
                },
                300_000,
                len - 300_000,
            ),
            0.1,
            0.36,
        ),
        Some(("slide_left_in", 0.6)),
        Some(("fade_out", 0.3)),
        None,
    );
    vec![
        (TrackKind::Overlay, bar),
        (TrackKind::Overlay, tag),
        (TrackKind::Text, tag_text),
        (TrackKind::Text, headline),
    ]
}

fn title_underline(c: &Canvas) -> Vec<(TrackKind, Clip)> {
    let len = c.len;
    let title = animated(
        c.text(
            "Big title",
            |t| {
                t.style.size = 140.0;
                t.style.bold = true;
                t.style.stroke = stroke(INK, 6.0);
            },
            0,
            len,
        ),
        Some(("blur_in", 0.7)),
        Some(("fade_out", 0.5)),
        None,
    );
    let line = animated(
        c.at(
            c.shape(ShapeKind::Line, 520.0, 12.0, YELLOW, 300_000, len - 300_000),
            0.0,
            0.1,
        ),
        Some(("wipe_in", 0.6)),
        Some(("fade_out", 0.4)),
        None,
    );
    vec![(TrackKind::Text, title), (TrackKind::Overlay, line)]
}

fn countdown(c: &Canvas) -> Vec<(TrackKind, Clip)> {
    let second = US_PER_SEC;
    ["3", "2", "1"]
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let digit = animated(
                c.text(
                    n,
                    |t| {
                        t.style.size = 360.0;
                        t.style.bold = true;
                        t.style.color = YELLOW;
                        t.style.stroke = stroke(INK, 10.0);
                    },
                    i as Micros * second,
                    second,
                ),
                Some(("pop_in", 0.35)),
                Some(("zoom_in_out", 0.3)),
                None,
            );
            (TrackKind::Text, digit)
        })
        .collect()
}

fn callout(c: &Canvas) -> Vec<(TrackKind, Clip)> {
    let len = c.len;
    let bubble = animated(
        c.at(
            c.shape(ShapeKind::Bubble, 560.0, 340.0, WHITE, 0, len),
            0.0,
            -0.05,
        ),
        Some(("pop_in", 0.5)),
        Some(("pop_out", 0.4)),
        Some(("float", 2.0)),
    );
    let words = animated(
        c.at(
            c.text(
                "Look here!",
                |t| {
                    t.style.size = 76.0;
                    t.style.bold = true;
                    t.style.color = INK;
                },
                0,
                len,
            ),
            0.0,
            -0.075,
        ),
        Some(("pop_in", 0.5)),
        Some(("pop_out", 0.4)),
        Some(("float", 2.0)),
    );
    vec![(TrackKind::Overlay, bubble), (TrackKind::Text, words)]
}

fn social_handle(c: &Canvas) -> Vec<(TrackKind, Clip)> {
    let len = c.len;
    let handle = animated(
        c.at(
            c.text(
                "@yourname",
                |t| {
                    t.style.size = 60.0;
                    t.style.bold = true;
                    t.style.color = INK;
                    t.style.background = Some(TextBackground {
                        color: YELLOW,
                        padding: 22.0,
                        radius: 40.0,
                    });
                },
                0,
                len,
            ),
            0.0,
            0.38,
        ),
        Some(("zoom_in", 0.5)),
        Some(("fade_out", 0.4)),
        None,
    );
    vec![(TrackKind::Text, handle)]
}

fn sinhala_card(c: &Canvas) -> Vec<(TrackKind, Clip)> {
    let len = c.len;
    let band = animated(
        c.at(
            c.rounded(1300.0, 300.0, 30.0, [20, 20, 20, 225], len),
            0.0,
            0.0,
        ),
        Some(("fade_in", 0.5)),
        Some(("fade_out", 0.5)),
        None,
    );
    let title = animated(
        c.at(
            c.text(
                "ශ්‍රී ලංකාව",
                |t| {
                    t.style.size = 130.0;
                    t.style.bold = true;
                    t.style.color = YELLOW;
                },
                300_000,
                len - 300_000,
            ),
            0.0,
            -0.03,
        ),
        Some(("slide_up_in", 0.7)),
        Some(("fade_out", 0.5)),
        None,
    );
    let sub = animated(
        c.at(
            c.text(
                "ඔබේ වචන මෙතැන ලියන්න",
                |t| {
                    t.style.size = 52.0;
                },
                600_000,
                len - 600_000,
            ),
            0.0,
            0.065,
        ),
        Some(("fade_in", 0.8)),
        Some(("fade_out", 0.5)),
        None,
    );
    vec![
        (TrackKind::Overlay, band),
        (TrackKind::Text, title),
        (TrackKind::Text, sub),
    ]
}

/// All built-in text templates, in the order the panel shows them.
pub fn templates() -> Vec<Template> {
    let t = |id, name, secs: f32, build| Template {
        id,
        name,
        duration: (secs * US_PER_SEC as f32) as Micros,
        build,
    };
    vec![
        t("lower_third", "Lower third", 4.0, lower_third),
        t("title_underline", "Title with line", 4.0, title_underline),
        t("quote_card", "Quote card", 5.0, quote_card),
        t("breaking_news", "Breaking news", 5.0, breaking_news),
        t("subscribe", "Subscribe button", 4.0, subscribe),
        t("social_handle", "Social name", 4.0, social_handle),
        t("callout", "Speech callout", 4.0, callout),
        t("countdown", "Countdown 3-2-1", 3.0, countdown),
        t("sinhala_card", "Sinhala title card", 5.0, sinhala_card),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_has_layers_that_start_inside_its_length_and_use_real_animations() {
        for t in templates() {
            let parts = t.build(1920, 1080);
            assert!(!parts.is_empty(), "{}", t.id);
            for (kind, clip) in &parts {
                assert!(
                    matches!(kind, TrackKind::Overlay | TrackKind::Text),
                    "{}",
                    t.id
                );
                assert!(
                    clip.start >= 0 && clip.start < t.duration,
                    "{} starts at {}",
                    t.id,
                    clip.start
                );
                assert!(
                    clip.end() <= t.duration + 1,
                    "{}: a layer ends after the template",
                    t.id
                );
                for a in [&clip.anim_in, &clip.anim_out, &clip.anim_combo]
                    .into_iter()
                    .flatten()
                {
                    assert!(
                        crate::animation::find(&a.preset).is_some(),
                        "{}: unknown animation {}",
                        t.id,
                        a.preset
                    );
                }
            }
        }
    }

    #[test]
    fn template_ids_are_unique_and_names_are_filled() {
        let all = templates();
        let mut ids: Vec<_> = all.iter().map(|t| t.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all.len());
        assert!(all.iter().all(|t| !t.name.is_empty()));
    }

    #[test]
    fn a_template_follows_the_canvas_shape() {
        let lower = templates()
            .into_iter()
            .find(|t| t.id == "lower_third")
            .unwrap();
        let wide = lower.build(1920, 1080);
        let tall = lower.build(1080, 1920);
        let y = |p: &Vec<(TrackKind, Clip)>| p[1].1.transform.y.value_at(0);
        assert!(
            y(&wide) > 0.0 && y(&tall) > y(&wide),
            "{} {}",
            y(&wide),
            y(&tall)
        );
    }

    #[test]
    fn the_sinhala_card_keeps_its_letters() {
        let t = templates()
            .into_iter()
            .find(|t| t.id == "sinhala_card")
            .unwrap();
        let parts = t.build(1920, 1080);
        let words: Vec<String> = parts
            .iter()
            .filter_map(|(_, c)| match &c.content {
                vz_core::Content::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect();
        assert!(words.iter().any(|w| w == "ශ්‍රී ලංකාව"));
    }
}
