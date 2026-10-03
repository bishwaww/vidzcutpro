//! Tests for gradients, strokes, glow, curves, case, spans, caption word effects and animators.

use super::*;
use vz_core::{
    BLACK, Gradient, StyleSpan, TextAnimator, TextCase, TextGlow, TextPhase, TextStroke,
    US_PER_SEC, WordMode,
};

fn base(text: &str) -> TextContent {
    let mut t = TextContent::new(text);
    t.style.size = 120.0;
    t.style.bold = true;
    t
}

fn at(img: &TextImage, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * img.width + x) * 4) as usize;
    [
        img.rgba[i],
        img.rgba[i + 1],
        img.rgba[i + 2],
        img.rgba[i + 3],
    ]
}

fn ink(img: &TextImage) -> u64 {
    img.rgba.chunks(4).map(|p| u64::from(p[3])).sum()
}

fn dump(name: &str, img: &TextImage) {
    if let Ok(dir) = std::env::var("VZ_TEXT_DUMP") {
        let _ = std::fs::create_dir_all(&dir);
        let mut flat = img.rgba.clone();
        for p in flat.chunks_mut(4) {
            let a = u32::from(p[3]);
            for c in &mut p[..3] {
                *c = ((u32::from(*c) * a + 40 * (255 - a)) / 255) as u8;
            }
            p[3] = 255;
        }
        image::save_buffer(
            format!("{dir}/{name}.png"),
            &flat,
            img.width,
            img.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

/// Average colour of the opaque-ish pixels in a range of columns.
fn colour_in(img: &TextImage, x0: u32, x1: u32) -> [f32; 3] {
    let (mut sum, mut n) = ([0.0f32; 3], 0.0f32);
    for y in 0..img.height {
        for x in x0..x1.min(img.width) {
            let p = at(img, x, y);
            if p[3] > 200 {
                for k in 0..3 {
                    sum[k] += f32::from(p[k]);
                }
                n += 1.0;
            }
        }
    }
    sum.map(|v| v / n.max(1.0))
}

#[test]
fn a_gradient_fills_the_letters_with_changing_colour() {
    let mut e = TextEngine::new();
    let mut t = base("MMMMMM");
    t.style.gradient = Some(Gradient::two([255, 0, 0, 255], [0, 0, 255, 255], 0.0));
    let img = e.render(&t, 1.0);
    dump("gradient", &img);
    let (third, last) = (img.width / 5, img.width * 4 / 5);
    let left = colour_in(&img, third / 2, third);
    let right = colour_in(&img, last, last + third / 2);
    assert!(left[0] > left[2] + 60.0, "left is red: {left:?}");
    assert!(right[2] > right[0] + 60.0, "right is blue: {right:?}");
}

#[test]
fn more_outlines_make_wider_rings_in_their_own_colours() {
    let mut e = TextEngine::new();
    let mut one = base("OOO");
    one.style.stroke = Some(TextStroke {
        color: BLACK,
        width: 6.0,
    });
    let mut three = one.clone();
    three.style.more_strokes = vec![
        TextStroke {
            color: [255, 0, 0, 255],
            width: 6.0,
        },
        TextStroke {
            color: [0, 0, 255, 255],
            width: 6.0,
        },
    ];
    let (a, b) = (e.render(&one, 1.0), e.render(&three, 1.0));
    dump("three_strokes", &b);
    assert!(ink(&b) > ink(&a) * 12 / 10);
    assert!(
        b.rgba
            .chunks(4)
            .any(|p| p[3] > 250 && p[0] > 240 && p[2] < 20),
        "a red ring"
    );
    assert!(
        b.rgba
            .chunks(4)
            .any(|p| p[3] > 250 && p[2] > 240 && p[0] < 20),
        "a blue ring"
    );
    assert!(b.width > a.width, "room is made for the rings");
}

#[test]
fn glow_adds_a_soft_halo_of_its_colour() {
    let mut e = TextEngine::new();
    let plain = base("Glow");
    let mut glowing = plain.clone();
    glowing.style.glow = Some(TextGlow {
        color: [255, 200, 0, 255],
        radius: 20.0,
        strength: 1.0,
    });
    let (a, b) = (e.render(&plain, 1.0), e.render(&glowing, 1.0));
    dump("glow", &b);
    assert!(ink(&b) > ink(&a) * 12 / 10);
    assert!(
        b.rgba
            .chunks(4)
            .any(|p| p[3] > 30 && p[3] < 200 && p[0] > 200 && p[2] < 60),
        "soft yellow pixels"
    );
}

#[test]
fn a_curve_bends_the_line_and_grows_the_picture() {
    let mut e = TextEngine::new();
    let straight = base("Curved line of text");
    let mut arch = straight.clone();
    arch.style.curve = 70.0;
    let (a, b) = (e.render(&straight, 1.0), e.render(&arch, 1.0));
    dump("curve", &b);
    assert!(b.height > a.height + 20, "{} vs {}", b.height, a.height);
    // the middle of the line is higher than its ends
    let top_of = |img: &TextImage, x: u32| {
        (0..img.height)
            .find(|y| at(img, x, *y)[3] > 100)
            .unwrap_or(u32::MAX)
    };
    assert!(
        top_of(&b, b.width / 2) + 10 < top_of(&b, b.width / 12),
        "{} {}",
        top_of(&b, b.width / 2),
        top_of(&b, b.width / 12)
    );
}

#[test]
fn case_changes_the_letters_but_not_sinhala() {
    let mut e = TextEngine::new();
    let mut t = base("shout");
    let small = e.measure(&t, 1.0).0;
    t.style.case = TextCase::Upper;
    assert!(e.measure(&t, 1.0).0 > small, "capitals are wider");
    let mut s = base("ශ්‍රී ලංකාව");
    let before = e.measure(&s, 1.0);
    s.style.case = TextCase::Upper;
    assert_eq!(e.measure(&s, 1.0), before);
}

#[test]
fn a_span_can_change_the_colour_and_size_of_part_of_the_text() {
    let mut e = TextEngine::new();
    let mut t = base("AB CD");
    t.style.color = [255, 255, 255, 255];
    t.spans = vec![StyleSpan {
        start: 3,
        end: 5,
        color: Some([255, 0, 0, 255]),
        size: Some(1.5),
        ..StyleSpan::default()
    }];
    let img = e.render(&t, 1.0);
    dump("span", &img);
    let plain = e.render(&base("AB CD"), 1.0);
    assert!(
        img.width > plain.width,
        "the bigger word makes the line longer"
    );
    let right = colour_in(&img, img.width * 2 / 3, img.width);
    let left = colour_in(&img, 0, img.width / 3);
    assert!(
        right[0] > right[1] + 80.0,
        "the last word is red: {right:?}"
    );
    assert!(left[1] > 200.0, "the first word is white: {left:?}");
}

#[test]
fn hidden_words_keep_their_room_but_draw_nothing() {
    let mut e = TextEngine::new();
    let mut t = base("one two");
    let full = e.render(&t, 1.0);
    t.spans = vec![StyleSpan {
        start: 4,
        end: 7,
        hidden: true,
        ..StyleSpan::default()
    }];
    let half = e.render(&t, 1.0);
    assert_eq!(full.width, half.width, "the room is kept");
    assert!(ink(&half) < ink(&full) * 7 / 10);
    let right_ink: u64 = (half.width / 2 + 20..half.width)
        .map(|x| {
            (0..half.height)
                .map(|y| u64::from(at(&half, x, y)[3]))
                .sum::<u64>()
        })
        .sum();
    assert_eq!(right_ink, 0, "nothing on the right");
}

#[test]
fn sinhala_spans_colour_whole_letter_groups() {
    let mut e = TextEngine::new();
    let mut t = base("ශ්‍රී ලංකාව");
    t.spans = vec![StyleSpan {
        start: 6,
        end: 11,
        color: Some([0, 255, 0, 255]),
        ..StyleSpan::default()
    }];
    let img = e.render(&t, 1.0);
    dump("sinhala_span", &img);
    let right = colour_in(&img, img.width / 2 + 30, img.width);
    assert!(right[1] > right[0] + 100.0, "{right:?}");
}

#[test]
fn karaoke_highlights_words_as_time_passes_and_reveal_adds_words() {
    let mut e = TextEngine::new();
    let mut t = base("one two three");
    t.style.color = [255, 255, 255, 255];
    t.words = WordMode::Karaoke {
        highlight: [255, 0, 0, 255],
        scale: 1.0,
    };
    let early = e.render_at(&t, 1.0, 0, 6 * US_PER_SEC);
    let late = e.render_at(&t, 1.0, 5 * US_PER_SEC, 6 * US_PER_SEC);
    let reds = |img: &TextImage| {
        img.rgba
            .chunks(4)
            .filter(|p| p[3] > 200 && p[0] > 200 && p[1] < 80)
            .count()
    };
    assert!(
        reds(&late) > reds(&early) * 2,
        "{} vs {}",
        reds(&late),
        reds(&early)
    );
    t.words = WordMode::Reveal { highlight: None };
    let first = e.render_at(&t, 1.0, 0, 6 * US_PER_SEC);
    let last = e.render_at(&t, 1.0, 5_900_000, 6 * US_PER_SEC);
    assert!(ink(&last) > ink(&first) * 2);
    assert_eq!(
        first.width, last.width,
        "the line is as wide from the start"
    );
}

#[test]
fn single_word_mode_shows_one_word_at_a_time() {
    let mut e = TextEngine::new();
    let mut t = base("alpha beta gamma");
    t.words = WordMode::Single { scale: 1.0 };
    let a = e.render_at(&t, 1.0, 0, 9 * US_PER_SEC);
    let b = e.render_at(&t, 1.0, 8 * US_PER_SEC, 9 * US_PER_SEC);
    let line = e.measure(&base("alpha beta gamma"), 1.0).0 as u32;
    assert!(a.width < line, "one word is narrower than the line");
    assert_ne!(a.width, b.width, "different words are different widths");
}

#[test]
fn an_animator_moves_the_letters_in_and_they_settle_in_place() {
    let mut e = TextEngine::new();
    let mut t = base("Hello");
    t.animators = vec![TextAnimator {
        dy: 80.0,
        opacity: 0.0,
        delay: 0.1,
        duration: 0.4,
        easing: vz_anim::Easing::Linear,
        ..TextAnimator::default()
    }];
    let dur = 4 * US_PER_SEC;
    let start = e.render_at(&t, 1.0, 0, dur);
    let mid = e.render_at(&t, 1.0, 300_000, dur);
    let done = e.render_at(&t, 1.0, 2 * US_PER_SEC, dur);
    dump("animator_mid", &mid);
    assert_eq!(
        ink(&start),
        0,
        "every letter is still invisible at the start"
    );
    assert!(
        ink(&mid) > 0 && ink(&mid) < ink(&done),
        "some letters have arrived"
    );
    // once finished it looks like the same text without animators
    let plain = e.render(&base("Hello"), 1.0);
    let (a, b) = (ink(&done), ink(&plain));
    assert!((a as f64 / b as f64 - 1.0).abs() < 0.03, "{a} vs {b}");
}

#[test]
fn animators_work_by_word_and_by_line_and_keep_sinhala_groups_whole() {
    let mut e = TextEngine::new();
    for unit in [
        vz_core::TextUnit::Word,
        vz_core::TextUnit::Line,
        vz_core::TextUnit::Character,
    ] {
        let mut t = base("ශ්‍රී ලංකාව\nක්‍රීඩාව");
        t.animators = vec![TextAnimator {
            unit,
            dy: 0.0,
            dx: 200.0,
            opacity: 1.0,
            ..TextAnimator::default()
        }];
        let img = e.render_at(&t, 1.0, 100_000, 3 * US_PER_SEC);
        assert!(ink(&img) > 10_000, "{unit:?}");
    }
    // a conjunct is one unit: it is not split into its characters
    let t = base("ශ්‍රී");
    let laid = e.lay_out(&t, 1.0);
    let glyphs: usize = laid.buffer.layout_runs().map(|r| r.glyphs.len()).sum();
    assert!(glyphs < 5);
}

#[test]
fn an_out_animator_makes_the_text_go_away_at_the_end() {
    let mut e = TextEngine::new();
    let mut t = base("Bye");
    t.animators = vec![TextAnimator {
        phase: TextPhase::Out,
        dy: 60.0,
        opacity: 0.0,
        delay: 0.0,
        duration: 0.5,
        ..TextAnimator::default()
    }];
    let dur = 2 * US_PER_SEC;
    let middle = e.render_at(&t, 1.0, US_PER_SEC, dur);
    let end = e.render_at(&t, 1.0, dur, dur);
    assert!(ink(&middle) > 0);
    assert_eq!(ink(&end), 0, "gone at the very end");
}

#[test]
fn spans_animators_and_gradients_survive_save_and_load() {
    let mut t = base("Saved");
    t.spans.push(StyleSpan {
        start: 0,
        end: 2,
        color: Some([1, 2, 3, 255]),
        ..StyleSpan::default()
    });
    t.animators.push(TextAnimator::default());
    t.words = WordMode::Karaoke {
        highlight: [9, 9, 9, 255],
        scale: 1.1,
    };
    t.style.gradient = Some(Gradient::two([0, 0, 0, 255], [255; 4], 45.0));
    t.style.more_strokes.push(TextStroke {
        color: BLACK,
        width: 3.0,
    });
    let back: TextContent = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
    assert_eq!(back, t);
    // old projects without any of the new fields still load
    let old: TextContent = serde_json::from_str(
        r#"{"text":"old","style":{"size":72.0,"color":[255,255,255,255],"line_spacing":1.0,"letter_spacing":0.0}}"#,
    )
    .unwrap();
    assert_eq!(old.text, "old");
    assert!(old.spans.is_empty() && old.animators.is_empty() && old.style.gradient.is_none());
}
