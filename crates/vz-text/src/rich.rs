//! Preparing text before it is laid out: letter case, word-by-word caption effects, and the looks of parts of the text.

use std::borrow::Cow;
use vz_core::{Color, Micros, StyleSpan, TextCase, TextContent, WordMode};

/// Apply upper / lower / title case one character at a time (so character counts, and with them the spans, stay the same).
pub fn apply_case(text: &str, case: TextCase) -> String {
    let one = |c: char, up: bool| -> char {
        if up {
            c.to_uppercase().next().unwrap_or(c)
        } else {
            c.to_lowercase().next().unwrap_or(c)
        }
    };
    match case {
        TextCase::Normal => text.to_owned(),
        TextCase::Upper => text.chars().map(|c| one(c, true)).collect(),
        TextCase::Lower => text.chars().map(|c| one(c, false)).collect(),
        TextCase::Title => {
            let mut start = true;
            text.chars()
                .map(|c| {
                    let out = one(c, start);
                    start = c.is_whitespace();
                    out
                })
                .collect()
        }
    }
}

/// The words of a text as character ranges (a word is anything between white space).
pub fn word_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    let mut count = 0;
    for (i, c) in text.chars().enumerate() {
        count = i + 1;
        match (c.is_whitespace(), start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                out.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, count));
    }
    out
}

/// When each word starts (microseconds from the start of the clip): the given times if there is one per word, otherwise
/// the clip's length shared out by the length of the words.
pub fn word_starts(text: &TextContent, words: &[(usize, usize)], duration: Micros) -> Vec<Micros> {
    if text.word_times.len() == words.len() && !words.is_empty() {
        return text.word_times.clone();
    }
    let total: usize = words.iter().map(|(s, e)| e - s).sum::<usize>().max(1);
    let mut before = 0;
    words
        .iter()
        .map(|(s, e)| {
            let t = (duration as f64 * before as f64 / total as f64) as Micros;
            before += e - s;
            t
        })
        .collect()
}

fn span(start: usize, end: usize) -> StyleSpan {
    StyleSpan {
        start,
        end,
        ..StyleSpan::default()
    }
}

/// The text as it should look `local` microseconds into a clip that lasts `duration`: caption word effects turned into
/// plain spans. Text without a word mode is returned as it is.
pub fn effective(text: &TextContent, local: Micros, duration: Micros) -> Cow<'_, TextContent> {
    if text.words == WordMode::Off {
        return Cow::Borrowed(text);
    }
    let words = word_ranges(&text.text);
    let starts = word_starts(text, &words, duration);
    // the word being spoken now: the last one that has started
    let current = starts.iter().rposition(|s| *s <= local).unwrap_or(0);
    let mut t = text.clone();
    t.words = WordMode::Off;
    match &text.words {
        WordMode::Off => {}
        WordMode::Karaoke { highlight, scale } => {
            for (i, (s, e)) in words.iter().enumerate() {
                if starts.get(i).is_some_and(|st| *st <= local) {
                    t.spans.push(StyleSpan {
                        color: Some(*highlight),
                        size: (i == current && (*scale - 1.0).abs() > 1e-3).then_some(*scale),
                        ..span(*s, *e)
                    });
                }
            }
        }
        WordMode::Reveal { highlight } => {
            for (i, (s, e)) in words.iter().enumerate() {
                if starts.get(i).is_some_and(|st| *st > local) {
                    t.spans.push(StyleSpan {
                        hidden: true,
                        ..span(*s, *e)
                    });
                } else if let Some(c) = highlight {
                    t.spans.push(StyleSpan {
                        color: Some(*c),
                        ..span(*s, *e)
                    });
                }
            }
        }
        WordMode::Single { scale } => {
            let chars: Vec<char> = text.text.chars().collect();
            if let Some((s, e)) = words.get(current) {
                t.text = chars[*s..*e].iter().collect();
                t.spans = vec![StyleSpan {
                    size: Some(*scale),
                    ..span(0, e - s)
                }];
            }
        }
        WordMode::Keywords {
            highlight,
            scale,
            words: keys,
        } => {
            for k in keys {
                if let Some((s, e)) = words.get(*k) {
                    t.spans.push(StyleSpan {
                        color: Some(*highlight),
                        size: ((*scale - 1.0).abs() > 1e-3).then_some(*scale),
                        ..span(*s, *e)
                    });
                }
            }
        }
    }
    Cow::Owned(t)
}

/// One stretch of text with one look.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub text: String,
    pub color: Option<Color>,
    pub size: f32,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub font: Option<String>,
}

/// Cut the text into stretches that each have one look (later spans win over earlier ones where they overlap).
/// Hidden parts get a see-through colour.
pub fn segments(text: &str, spans: &[StyleSpan], base: Color) -> Vec<Segment> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut cuts: Vec<usize> = vec![0, n];
    for s in spans {
        cuts.push(s.start.min(n));
        cuts.push(s.end.min(n));
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut out: Vec<Segment> = Vec::new();
    for pair in cuts.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if a == b {
            continue;
        }
        let mut seg = Segment {
            text: chars[a..b].iter().collect(),
            color: None,
            size: 1.0,
            bold: None,
            italic: None,
            font: None,
        };
        for s in spans.iter().filter(|s| s.start <= a && s.end >= b) {
            if let Some(c) = s.color {
                seg.color = Some(c);
            }
            if let Some(z) = s.size {
                seg.size = z.clamp(0.1, 10.0);
            }
            if s.bold.is_some() {
                seg.bold = s.bold;
            }
            if s.italic.is_some() {
                seg.italic = s.italic;
            }
            if s.font.is_some() {
                seg.font = s.font.clone();
            }
            if s.hidden {
                let c = seg.color.unwrap_or(base);
                seg.color = Some([c[0], c[1], c[2], 0]);
            }
        }
        // join neighbours that look the same
        match out.last_mut() {
            Some(prev)
                if (prev.color, prev.size, &prev.bold, &prev.italic, &prev.font)
                    == (seg.color, seg.size, &seg.bold, &seg.italic, &seg.font) =>
            {
                prev.text.push_str(&seg.text);
            }
            _ => out.push(seg),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use vz_core::US_PER_SEC;

    #[test]
    fn case_changes_keep_the_number_of_characters() {
        assert_eq!(apply_case("hello world", TextCase::Upper), "HELLO WORLD");
        assert_eq!(apply_case("HeLLo", TextCase::Lower), "hello");
        assert_eq!(
            apply_case("hello big world", TextCase::Title),
            "Hello Big World"
        );
        // Sinhala and Tamil have no case: they come back unchanged
        assert_eq!(apply_case("ශ්‍රී ලංකාව", TextCase::Upper), "ශ්‍රී ලංකාව");
        assert_eq!(apply_case("ශ්‍රී", TextCase::Title).chars().count(), 5);
    }

    #[test]
    fn words_are_found_between_white_space() {
        assert_eq!(word_ranges("ab cd  e"), vec![(0, 2), (3, 5), (7, 8)]);
        assert_eq!(word_ranges(""), vec![]);
        assert_eq!(word_ranges("  x "), vec![(2, 3)]);
        // a Sinhala word is counted in characters (code points), not bytes
        assert_eq!(word_ranges("ශ්‍රී ලංකාව"), vec![(0, 5), (6, 11)]);
    }

    fn caption(words: WordMode) -> TextContent {
        TextContent {
            text: "one two three four".into(),
            words,
            ..TextContent::default()
        }
    }

    #[test]
    fn words_share_the_clip_by_their_length_unless_times_are_given() {
        let mut t = caption(WordMode::Off);
        let words = word_ranges(&t.text);
        let starts = word_starts(&t, &words, 14 * US_PER_SEC);
        // 3 + 3 + 5 + 4 = 15 letters: the first word starts at 0, the third after 6 of 15
        assert_eq!(starts[0], 0);
        assert_eq!(starts[2], 14 * US_PER_SEC * 6 / 15);
        t.word_times = vec![0, 100, 200, 300];
        assert_eq!(
            word_starts(&t, &words, 14 * US_PER_SEC),
            vec![0, 100, 200, 300]
        );
        // the wrong number of times is ignored
        t.word_times = vec![0, 100];
        assert_eq!(word_starts(&t, &words, 14 * US_PER_SEC)[0], 0);
        assert_eq!(word_starts(&t, &words, 14 * US_PER_SEC).len(), 4);
    }

    #[test]
    fn karaoke_colours_the_words_spoken_so_far() {
        let red = [255, 0, 0, 255];
        let t = caption(WordMode::Karaoke {
            highlight: red,
            scale: 1.2,
        });
        let at = |local: Micros| effective(&t, local, 15 * US_PER_SEC).into_owned();
        assert!(
            at(0).spans.iter().filter(|s| s.color == Some(red)).count() == 1,
            "only the first word has started"
        );
        let later = at(7 * US_PER_SEC);
        let coloured: Vec<_> = later
            .spans
            .iter()
            .filter(|s| s.color == Some(red))
            .collect();
        assert_eq!(
            coloured.len(),
            3,
            "one, two, three have been spoken (7 s is after six letters)"
        );
        assert_eq!(
            coloured.iter().filter(|s| s.size.is_some()).count(),
            1,
            "only the word being spoken is bigger"
        );
        assert_eq!(later.words, WordMode::Off);
    }

    #[test]
    fn reveal_hides_the_words_that_have_not_come_yet_and_single_shows_one_word() {
        let t = caption(WordMode::Reveal { highlight: None });
        let e = effective(&t, 2 * US_PER_SEC, 15 * US_PER_SEC).into_owned();
        let hidden: Vec<_> = e.spans.iter().filter(|s| s.hidden).collect();
        assert_eq!(
            hidden.len(),
            3,
            "everything but the first word is still hidden"
        );
        let segs = segments(&e.text, &e.spans, [255; 4]);
        assert!(segs.iter().any(|s| s.color.is_some_and(|c| c[3] == 0)));
        let single = effective(
            &caption(WordMode::Single { scale: 1.3 }),
            10 * US_PER_SEC,
            15 * US_PER_SEC,
        )
        .into_owned();
        assert_eq!(single.text, "three");
        assert_eq!(single.spans[0].size, Some(1.3));
        let before = effective(
            &caption(WordMode::Single { scale: 1.0 }),
            -5,
            15 * US_PER_SEC,
        )
        .into_owned();
        assert_eq!(
            before.text, "one",
            "before the first word the first word shows"
        );
    }

    #[test]
    fn keywords_always_stand_out() {
        let t = caption(WordMode::Keywords {
            highlight: [255, 64, 64, 255],
            scale: 1.2,
            words: vec![1, 3],
        });
        let e = effective(&t, 0, US_PER_SEC).into_owned();
        assert_eq!(e.spans.len(), 2);
        assert_eq!((e.spans[0].start, e.spans[0].end), (4, 7));
        assert_eq!((e.spans[1].start, e.spans[1].end), (14, 18));
        // a word that does not exist is ignored
        let bad = caption(WordMode::Keywords {
            highlight: [1; 4],
            scale: 1.0,
            words: vec![9],
        });
        assert!(effective(&bad, 0, US_PER_SEC).spans.is_empty());
    }

    #[test]
    fn plain_text_is_not_copied() {
        let t = caption(WordMode::Off);
        assert!(matches!(effective(&t, 0, 1), Cow::Borrowed(_)));
    }

    #[test]
    fn segments_cut_the_text_where_the_look_changes_and_later_spans_win() {
        let spans = vec![
            StyleSpan {
                start: 0,
                end: 5,
                color: Some([1, 1, 1, 255]),
                ..StyleSpan::default()
            },
            StyleSpan {
                start: 3,
                end: 8,
                color: Some([2, 2, 2, 255]),
                bold: Some(true),
                ..StyleSpan::default()
            },
        ];
        let segs = segments("abcdefghij", &spans, [255; 4]);
        let texts: Vec<_> = segs.iter().map(|s| s.text.as_str()).collect();
        // "de" and "fgh" look the same (the later span wins over the first on "de"), so they are one piece
        assert_eq!(texts, ["abc", "defgh", "ij"]);
        assert_eq!(segs[1].color, Some([2, 2, 2, 255]));
        assert_eq!(segs[0].bold, None);
        assert_eq!(segs[1].bold, Some(true));
        // spans that go past the end are clipped, and no spans give one piece
        assert_eq!(
            segments(
                "abc",
                &[StyleSpan {
                    start: 1,
                    end: 99,
                    ..StyleSpan::default()
                }],
                [0; 4]
            )
            .len(),
            1
        );
        assert_eq!(segments("abc", &[], [0; 4]).len(), 1);
        assert!(segments("", &[], [0; 4]).is_empty());
    }
}
