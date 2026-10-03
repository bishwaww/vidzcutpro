//! Caption word effects and text animators, seen through the whole renderer.

use super::*;
use vz_core::{
    CaptionLook, Document, TextAnimator, TextPhase, TextStyle, TextUnit, US_PER_SEC, WordMode,
};
use vz_media::testutil::{self, Pattern, Spec};

const SIZE: (u32, u32) = (640, 360);
const YELLOW: [u8; 4] = [255, 220, 0, 255];

fn dark_video() -> PathBuf {
    testutil::video(
        "cap_dark_640x360.mp4",
        Spec {
            pattern: Pattern::Solid(0x101030),
            width: 640,
            height: 360,
            audio_hz: None,
            seconds: 6,
            ..Spec::default()
        },
    )
}

fn doc_with_caption(words: WordMode, animators: Vec<TextAnimator>) -> Document {
    let mut doc = Document::new(Project::new("captions"));
    let m = doc
        .import_media(vz_media::probe(&dark_video()).unwrap())
        .unwrap();
    doc.add_clip(m, 0).unwrap();
    let style = TextStyle {
        color: [255, 255, 255, 255],
        size: 110.0,
        bold: true,
        ..TextStyle::default()
    };
    let look = CaptionLook {
        style,
        words,
        animators,
        y: 0.0,
    };
    doc.add_caption(US_PER_SEC, 4 * US_PER_SEC, "one two three four", &look)
        .unwrap();
    doc
}

fn count(f: &VideoFrame, test: impl Fn(&[u8]) -> bool) -> usize {
    f.rgba.chunks(4).filter(|p| test(p)).count()
}

fn is_yellow(p: &[u8]) -> bool {
    p[0] > 200 && p[1] > 170 && p[2] < 90
}

fn is_white(p: &[u8]) -> bool {
    p[0] > 200 && p[1] > 200 && p[2] > 200
}

#[test]
fn karaoke_colours_more_words_as_time_goes_by() {
    let doc = doc_with_caption(
        WordMode::Karaoke {
            highlight: YELLOW,
            scale: 1.0,
        },
        Vec::new(),
    );
    let mut r = Renderer::new();
    let early = r.render(&doc.project, US_PER_SEC + US_PER_SEC / 10, SIZE);
    let late = r.render(&doc.project, 4 * US_PER_SEC + US_PER_SEC / 2, SIZE);
    let (y_early, y_late) = (count(&early, is_yellow), count(&late, is_yellow));
    assert!(
        y_late > y_early * 2,
        "yellow pixels: early {y_early}, late {y_late}"
    );
    // the words that have not been said yet are still white at the start
    assert!(count(&early, is_white) > count(&late, is_white));
}

#[test]
fn single_word_mode_shows_one_word_at_a_time() {
    let single = doc_with_caption(WordMode::Single { scale: 1.0 }, Vec::new());
    let all = doc_with_caption(WordMode::Off, Vec::new());
    let mut r = Renderer::new();
    let t = 2 * US_PER_SEC;
    let one = count(&r.render(&single.project, t, SIZE), is_white);
    let whole = count(&r.render(&all.project, t, SIZE), is_white);
    assert!(one > 100, "a word is shown");
    assert!(one < whole * 7 / 10, "{one} of {whole}");
}

#[test]
fn reveal_mode_adds_words_over_time() {
    let doc = doc_with_caption(WordMode::Reveal { highlight: None }, Vec::new());
    let mut r = Renderer::new();
    let early = count(
        &r.render(&doc.project, US_PER_SEC + US_PER_SEC / 10, SIZE),
        is_white,
    );
    let late = count(
        &r.render(&doc.project, 4 * US_PER_SEC + US_PER_SEC / 2, SIZE),
        is_white,
    );
    assert!(late > early * 2, "{early} then {late}");
}

#[test]
fn a_per_letter_fade_in_starts_empty_and_ends_full() {
    let animator = TextAnimator {
        unit: TextUnit::Character,
        phase: TextPhase::In,
        opacity: 0.0,
        delay: 0.05,
        duration: 0.3,
        ..TextAnimator::default()
    };
    let doc = doc_with_caption(WordMode::Off, vec![animator]);
    let mut r = Renderer::new();
    let start = count(&r.render(&doc.project, US_PER_SEC, SIZE), is_white);
    let end = count(&r.render(&doc.project, 3 * US_PER_SEC, SIZE), is_white);
    assert!(start * 10 < end, "start {start}, end {end}");
}
