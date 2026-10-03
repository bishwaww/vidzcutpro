//! Reading and writing caption files: SRT, WebVTT, LRC, ASS (basic) and plain text.
//!
//! A caption is a line of text with a start and an end time. Files are read leniently (byte order marks, Windows
//! line ends, missing numbers, hours left out) because captions come from many programs.

use crate::time::{Micros, US_PER_SEC};

/// One caption.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptionLine {
    pub start: Micros,
    pub end: Micros,
    pub text: String,
}

/// The file types we can read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Srt,
    Vtt,
    Lrc,
    Ass,
    Txt,
}

impl Format {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "srt" => Some(Self::Srt),
            "vtt" => Some(Self::Vtt),
            "lrc" => Some(Self::Lrc),
            "ass" | "ssa" => Some(Self::Ass),
            "txt" => Some(Self::Txt),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum CaptionError {
    /// Nothing that looks like a caption was found.
    Empty,
}

impl std::fmt::Display for CaptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "no captions were found in the file"),
        }
    }
}

fn clean(text: &str) -> String {
    text.trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

/// Take out `<i>`, `<b>`, `<font ...>` style tags and `{\an8}` style override blocks.
fn strip_tags(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut depth_angle = false;
    let mut depth_brace = false;
    for c in line.chars() {
        match c {
            '<' if !depth_brace => depth_angle = true,
            '>' if depth_angle => depth_angle = false,
            '{' if !depth_angle => depth_brace = true,
            '}' if depth_brace => depth_brace = false,
            _ if depth_angle || depth_brace => {}
            _ => out.push(c),
        }
    }
    out
}

/// Read a time such as `00:01:02,500`, `01:02.5`, `1:02:03.45` or `62.5`.
pub fn parse_time(text: &str) -> Option<Micros> {
    let text = text.trim();
    let (clock, fraction) = match text.rfind([',', '.']) {
        Some(i) => (&text[..i], &text[i + 1..]),
        None => (text, ""),
    };
    if !fraction.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut seconds: i64 = 0;
    let parts: Vec<&str> = clock.split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    for part in &parts {
        seconds = seconds * 60 + part.trim().parse::<i64>().ok()?;
    }
    let frac = if fraction.is_empty() {
        0
    } else {
        // "5" is half a second, "05" a twentieth, "500" half a second
        let digits: String = fraction.chars().take(6).collect();
        let value: i64 = digits.parse().ok()?;
        value * 10_i64.pow(6 - digits.len() as u32)
    };
    Some(seconds * US_PER_SEC + frac)
}

/// `HH:MM:SS,mmm` (SRT) or with a dot (WebVTT).
pub fn format_time(t: Micros, separator: char) -> String {
    let ms = (t.max(0) + 500) / 1000;
    let (h, m, s, ms) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    format!("{h:02}:{m:02}:{s:02}{separator}{ms:03}")
}

fn parse_timing_line(line: &str) -> Option<(Micros, Micros)> {
    let (a, b) = line.split_once("-->")?;
    let start = parse_time(a)?;
    // WebVTT may add settings after the end time ("align:start position:0%")
    let end = parse_time(b.trim().split_whitespace().next()?)?;
    Some((start, end))
}

fn parse_blocks(text: &str) -> Vec<CaptionLine> {
    let text = clean(text);
    let mut out = Vec::new();
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.lines().map(str::trim_end).collect();
        let Some(i) = lines.iter().position(|l| l.contains("-->")) else {
            continue;
        };
        let Some((start, end)) = parse_timing_line(lines[i]) else {
            continue;
        };
        let body: Vec<String> = lines[i + 1..]
            .iter()
            .map(|l| strip_tags(l).trim().to_owned())
            .filter(|l| !l.is_empty())
            .collect();
        if body.is_empty() {
            continue;
        }
        out.push(CaptionLine {
            start,
            end: end.max(start),
            text: body.join("\n"),
        });
    }
    out
}

pub fn parse_srt(text: &str) -> Vec<CaptionLine> {
    parse_blocks(text)
}

pub fn parse_vtt(text: &str) -> Vec<CaptionLine> {
    parse_blocks(text)
}

/// LRC: `[mm:ss.xx]words`. A line may have several time stamps; a caption ends where the next one starts.
pub fn parse_lrc(text: &str) -> Vec<CaptionLine> {
    let text = clean(text);
    let mut stamped: Vec<(Micros, String)> = Vec::new();
    for line in text.lines() {
        let mut rest = line.trim();
        let mut times = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(close) = stripped.find(']') else {
                break;
            };
            match parse_time(&stripped[..close]) {
                Some(t)
                    if stripped[..close].contains(':')
                        && stripped[..close]
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_digit()) =>
                {
                    times.push(t)
                }
                _ => {
                    // a tag such as [ar:Singer]: not a time stamp
                    if times.is_empty() {
                        rest = "";
                    }
                    break;
                }
            }
            rest = stripped[close + 1..].trim_start();
        }
        let words = strip_tags(rest).trim().to_owned();
        if !words.is_empty() {
            for t in times {
                stamped.push((t, words.clone()));
            }
        }
    }
    stamped.sort_by_key(|(t, _)| *t);
    let mut out = Vec::new();
    for (i, (start, text)) in stamped.iter().enumerate() {
        let end = stamped
            .get(i + 1)
            .map_or(start + 3 * US_PER_SEC, |(t, _)| *t);
        out.push(CaptionLine {
            start: *start,
            end: end.max(*start),
            text: text.clone(),
        });
    }
    out
}

/// ASS / SSA: the `Dialogue:` lines of the `[Events]` section (only the text and times; styling is ignored).
pub fn parse_ass(text: &str) -> Vec<CaptionLine> {
    let text = clean(text);
    let mut in_events = false;
    let (mut start_i, mut end_i, mut text_i, mut fields) = (1usize, 2usize, 9usize, 10usize);
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_events = line.eq_ignore_ascii_case("[events]");
            continue;
        }
        if !in_events {
            continue;
        }
        if let Some(format) = line.strip_prefix("Format:") {
            let names: Vec<String> = format
                .split(',')
                .map(|n| n.trim().to_ascii_lowercase())
                .collect();
            fields = names.len();
            start_i = names.iter().position(|n| n == "start").unwrap_or(1);
            end_i = names.iter().position(|n| n == "end").unwrap_or(2);
            text_i = names
                .iter()
                .position(|n| n == "text")
                .unwrap_or(fields.saturating_sub(1));
        } else if let Some(dialogue) = line.strip_prefix("Dialogue:") {
            // the text is the last field and may itself contain commas
            let parts: Vec<&str> = dialogue.splitn(fields.max(text_i + 1), ',').collect();
            let (Some(s), Some(e), Some(body)) =
                (parts.get(start_i), parts.get(end_i), parts.get(text_i))
            else {
                continue;
            };
            let (Some(start), Some(end)) = (parse_time(s), parse_time(e)) else {
                continue;
            };
            let text = strip_tags(&body.replace("\\N", "\n").replace("\\n", "\n"))
                .trim()
                .to_owned();
            if !text.is_empty() {
                out.push(CaptionLine {
                    start,
                    end: end.max(start),
                    text,
                });
            }
        }
    }
    out
}

/// Plain text: one caption per non-empty line, spread evenly over `total` (or two seconds each if `total` is 0).
pub fn parse_txt(text: &str, total: Micros) -> Vec<CaptionLine> {
    let text = clean(text);
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        return Vec::new();
    }
    let each = if total > 0 {
        total / lines.len() as i64
    } else {
        2 * US_PER_SEC
    };
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| CaptionLine {
            start: each * i as i64,
            end: each * (i as i64 + 1),
            text: (*l).to_owned(),
        })
        .collect()
}

/// Read a caption file's text. `total` is only used for plain text files.
pub fn parse(text: &str, format: Format, total: Micros) -> Result<Vec<CaptionLine>, CaptionError> {
    let mut lines = match format {
        Format::Srt => parse_srt(text),
        Format::Vtt => parse_vtt(text),
        Format::Lrc => parse_lrc(text),
        Format::Ass => parse_ass(text),
        Format::Txt => parse_txt(text, total),
    };
    if lines.is_empty() {
        return Err(CaptionError::Empty);
    }
    lines.sort_by_key(|l| l.start);
    Ok(lines)
}

pub fn write_srt(lines: &[CaptionLine]) -> String {
    let mut out = String::new();
    for (i, l) in lines.iter().enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            format_time(l.start, ','),
            format_time(l.end, ','),
            l.text
        ));
    }
    out
}

pub fn write_vtt(lines: &[CaptionLine]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for l in lines {
        out.push_str(&format!(
            "{} --> {}\n{}\n\n",
            format_time(l.start, '.'),
            format_time(l.end, '.'),
            l.text
        ));
    }
    out
}

pub fn write_txt(lines: &[CaptionLine]) -> String {
    lines
        .iter()
        .map(|l| l.text.replace('\n', " "))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Cut every caption with more than `words` words into several, sharing its time by the length of the words.
pub fn split_by_words(lines: &[CaptionLine], words: usize) -> Vec<CaptionLine> {
    let words = words.max(1);
    let mut out = Vec::new();
    for l in lines {
        let parts: Vec<&str> = l.text.split_whitespace().collect();
        if parts.len() <= words {
            out.push(l.clone());
            continue;
        }
        let chunks: Vec<String> = parts.chunks(words).map(|c| c.join(" ")).collect();
        let total: usize = chunks
            .iter()
            .map(|c| c.chars().count())
            .sum::<usize>()
            .max(1);
        let span = l.end - l.start;
        let mut before = 0;
        for chunk in &chunks {
            let start = l.start + (span as f64 * before as f64 / total as f64) as Micros;
            before += chunk.chars().count();
            let end = l.start + (span as f64 * before as f64 / total as f64) as Micros;
            out.push(CaptionLine {
                start,
                end,
                text: chunk.clone(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRT: &str = "\u{feff}1\r\n00:00:01,000 --> 00:00:03,500\r\nHello <i>world</i>\r\n\r\n2\r\n00:00:04,000 --> 00:00:06,000\r\nශ්‍රී ලංකාව\r\nsecond line\r\n\r\n";

    #[test]
    fn times_are_read_in_every_common_shape() {
        assert_eq!(parse_time("00:00:01,500"), Some(1_500_000));
        assert_eq!(parse_time("01:02.5"), Some(62_500_000));
        assert_eq!(parse_time("1:02:03.45"), Some(3_723_450_000));
        assert_eq!(parse_time("62"), Some(62_000_000));
        assert_eq!(parse_time("0:00:00.05"), Some(50_000));
        assert_eq!(parse_time("00:00:00,5"), Some(500_000));
        assert_eq!(parse_time("abc"), None);
        assert_eq!(parse_time("1:2:3:4"), None);
        assert_eq!(format_time(3_723_450_000, ','), "01:02:03,450");
        assert_eq!(format_time(999, '.'), "00:00:00.001");
    }

    #[test]
    fn srt_is_read_with_bom_windows_line_ends_tags_and_two_line_captions() {
        let lines = parse_srt(SRT);
        assert_eq!(lines.len(), 2);
        assert_eq!((lines[0].start, lines[0].end), (1_000_000, 3_500_000));
        assert_eq!(lines[0].text, "Hello world");
        assert_eq!(lines[1].text, "ශ්‍රී ලංකාව\nsecond line");
    }

    #[test]
    fn srt_without_numbers_and_with_odd_blank_lines_still_reads() {
        let lines =
            parse_srt("00:00:01,000 --> 00:00:02,000\nA\n\n\n\n00:00:02,000 --> 00:00:03,000\nB\n");
        assert_eq!(
            lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
            ["A", "B"]
        );
        assert!(parse_srt("not a caption file").is_empty());
        // a caption with no words is skipped
        assert_eq!(parse_srt("1\n00:00:01,000 --> 00:00:02,000\n\n").len(), 0);
    }

    #[test]
    fn writing_srt_and_reading_it_back_gives_the_same_captions() {
        let lines = parse_srt(SRT);
        let again = parse_srt(&write_srt(&lines));
        assert_eq!(lines, again);
        assert!(write_srt(&lines).starts_with("1\n00:00:01,000 --> 00:00:03,500\nHello world"));
    }

    #[test]
    fn webvtt_has_a_header_notes_ids_and_settings() {
        let vtt = "WEBVTT\n\nNOTE this is a note\n\nintro\n00:01.000 --> 00:03.000 align:start position:0%\nHi there\n\n00:04.000 --> 00:05.000\n<v Roger>Hello\n";
        let lines = parse_vtt(vtt);
        assert_eq!(lines.len(), 2);
        assert_eq!((lines[0].start, lines[0].end), (1_000_000, 3_000_000));
        assert_eq!(lines[0].text, "Hi there");
        assert_eq!(lines[1].text, "Hello");
        let back = parse_vtt(&write_vtt(&lines));
        assert_eq!(back, lines);
        assert!(write_vtt(&lines).starts_with("WEBVTT\n\n00:00:01.000 --> 00:00:03.000"));
    }

    #[test]
    fn lrc_lines_end_where_the_next_begins_and_tags_are_ignored() {
        let lrc = "[ar:Someone]\n[ti:A song]\n[00:01.00]first line\n[00:04.50][01:00.00]second line\n[00:08.00]third <00:08.50>line\n";
        let lines = parse_lrc(lrc);
        let texts: Vec<_> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            texts,
            ["first line", "second line", "third line", "second line"]
        );
        assert_eq!((lines[0].start, lines[0].end), (1_000_000, 4_500_000));
        assert_eq!(lines[3].start, 60_000_000);
        assert_eq!(
            lines[3].end - lines[3].start,
            3_000_000,
            "the last line gets three seconds"
        );
    }

    #[test]
    fn ass_dialogue_lines_are_read_with_commas_in_the_text_and_override_tags() {
        let ass = "[Script Info]\nTitle: x\n\n[V4+ Styles]\nFormat: Name, Fontname\nStyle: Default,Arial\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.50,Default,,0,0,0,,{\\an8}Well, hello\\Nthere\nDialogue: 0,0:00:04.00,0:00:05.00,Default,,0,0,0,,ශ්‍රී ලංකාව\nComment: 0,0:00:06.00,0:00:07.00,Default,,0,0,0,,ignored\n";
        let lines = parse_ass(ass);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "Well, hello\nthere");
        assert_eq!((lines[0].start, lines[0].end), (1_000_000, 3_500_000));
        assert_eq!(lines[1].text, "ශ්‍රී ලංකාව");
    }

    #[test]
    fn plain_text_is_spread_evenly() {
        let lines = parse_txt("one\n\n two \nthree\n", 9 * US_PER_SEC);
        assert_eq!(lines.len(), 3);
        assert_eq!(
            (lines[1].start, lines[1].end, lines[1].text.as_str()),
            (3_000_000, 6_000_000, "two")
        );
        assert_eq!(parse_txt("a\nb", 0)[1].start, 2 * US_PER_SEC);
        assert_eq!(write_txt(&lines), "one\ntwo\nthree\n");
        assert!(parse_txt("  \n\n", 5).is_empty());
    }

    #[test]
    fn parse_picks_the_reader_from_the_format_and_refuses_empty_files() {
        assert_eq!(parse(SRT, Format::Srt, 0).unwrap().len(), 2);
        assert_eq!(parse("junk", Format::Srt, 0), Err(CaptionError::Empty));
        // captions come out in time order
        let unordered =
            "2\n00:00:05,000 --> 00:00:06,000\nlate\n\n1\n00:00:01,000 --> 00:00:02,000\nearly\n";
        let lines = parse(unordered, Format::Srt, 0).unwrap();
        assert_eq!(lines[0].text, "early");
        assert_eq!(Format::from_extension("SRT"), Some(Format::Srt));
        assert_eq!(Format::from_extension("ssa"), Some(Format::Ass));
        assert_eq!(Format::from_extension("mp4"), None);
    }

    #[test]
    fn long_captions_can_be_cut_into_a_few_words_each() {
        let lines = vec![CaptionLine {
            start: 0,
            end: 10 * US_PER_SEC,
            text: "one two three four five six seven".into(),
        }];
        let cut = split_by_words(&lines, 3);
        let texts: Vec<_> = cut.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["one two three", "four five six", "seven"]);
        // they cover the same time without gaps
        assert_eq!(cut[0].start, 0);
        assert_eq!(cut[0].end, cut[1].start);
        assert_eq!(cut[2].end, 10 * US_PER_SEC);
        // short ones stay as they are
        assert_eq!(split_by_words(&lines, 20), lines);
        assert_eq!(
            split_by_words(&lines, 0).len(),
            7,
            "zero means one word at a time"
        );
    }
}
