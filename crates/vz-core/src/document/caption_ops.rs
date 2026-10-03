//! Captions on the timeline: making them from lines of text, restyling, merging, cutting and shifting.

use super::{Command, DocError, Document, EditError};
use crate::captions::CaptionLine;
use crate::content::{Content, TextAnimator, TextContent, TextStyle, WordMode};
use crate::ids::{ClipId, TrackId};
use crate::time::{FrameRate, Micros};
use crate::timeline::{Clip, Track, TrackKind};

/// How captions look and where they sit.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptionLook {
    pub style: TextStyle,
    pub words: WordMode,
    pub animators: Vec<TextAnimator>,
    /// Vertical position from the middle of the canvas, as a share of the canvas height (0.3 = low on the picture).
    pub y: f32,
}

impl Default for CaptionLook {
    fn default() -> Self {
        Self {
            style: TextStyle {
                size: 64.0,
                bold: true,
                ..TextStyle::default()
            },
            words: WordMode::Off,
            animators: Vec::new(),
            y: 0.3,
        }
    }
}

/// A keyword look with no words chosen picks the longest word of the caption.
fn with_keywords(words: &WordMode, text: &str) -> WordMode {
    match words {
        WordMode::Keywords {
            highlight,
            scale,
            words,
        } if words.is_empty() => {
            let longest = text
                .split_whitespace()
                .enumerate()
                .max_by_key(|(_, w)| w.chars().count())
                .map(|(i, _)| i);
            WordMode::Keywords {
                highlight: *highlight,
                scale: *scale,
                words: longest.into_iter().collect(),
            }
        }
        other => other.clone(),
    }
}

fn caption_clip(line: &CaptionLine, look: &CaptionLook, rate: FrameRate, canvas_h: u32) -> Clip {
    let start = rate.snap(line.start.max(0));
    let end = rate.snap(line.end).max(start + rate.frame_start(1));
    let text = TextContent {
        text: line.text.clone(),
        style: look.style.clone(),
        animators: look.animators.clone(),
        words: with_keywords(&look.words, &line.text),
        ..TextContent::default()
    };
    let mut clip = Clip::text_clip(text, start, end - start);
    clip.transform.y = (look.y * canvas_h as f32).into();
    clip
}

impl Document {
    /// The track that holds the captions, if there is one.
    pub fn caption_track(&self) -> Option<TrackId> {
        self.project
            .timeline()
            .tracks
            .iter()
            .find(|t| t.captions)
            .map(|t| t.id)
    }

    /// The captions of a track in time order.
    pub fn caption_lines(&self, track: TrackId) -> Vec<(ClipId, CaptionLine)> {
        let Some(t) = self.project.timeline().track(track) else {
            return Vec::new();
        };
        let mut out: Vec<(ClipId, CaptionLine)> = t
            .clips
            .iter()
            .filter_map(|c| match &c.content {
                Content::Text(tc) => Some((
                    c.id,
                    CaptionLine {
                        start: c.start,
                        end: c.end(),
                        text: tc.text.clone(),
                    },
                )),
                _ => None,
            })
            .collect();
        out.sort_by_key(|(_, l)| l.start);
        out
    }

    /// Put captions on the captions track (made if there is none), replacing what is there. Overlaps are trimmed so each
    /// caption ends where the next begins. One undo step.
    pub fn set_captions(
        &mut self,
        lines: &[CaptionLine],
        look: &CaptionLook,
    ) -> Result<(TrackId, Vec<ClipId>), DocError> {
        let rate = self.project.frame_rate;
        let canvas_h = self.project.canvas.height;
        let mut sorted: Vec<CaptionLine> = lines.to_vec();
        sorted.sort_by_key(|l| l.start);
        for i in 0..sorted.len().saturating_sub(1) {
            let next = sorted[i + 1].start;
            if sorted[i].end > next {
                sorted[i].end = next.max(sorted[i].start + 1);
            }
        }
        let clips: Vec<Clip> = sorted
            .iter()
            .map(|l| caption_clip(l, look, rate, canvas_h))
            .collect();
        let ids: Vec<ClipId> = clips.iter().map(|c| c.id).collect();
        let tl = self.project.timeline();
        if let Some(track) = tl.tracks.iter().find(|t| t.captions) {
            let track_id = track.id;
            let command = Command::EditTrack {
                track: track_id,
                before: track.clips.clone(),
                after: clips,
                label: "Captions",
            };
            self.run(command, None)?;
            return Ok((track_id, ids));
        }
        let mut track = Track::new(TrackKind::Text, "Captions");
        track.captions = true;
        let track_id = track.id;
        let commands = vec![
            Command::AddTrack {
                track: Box::new(track),
                index: tl.tracks.len(),
            },
            Command::EditTrack {
                track: track_id,
                before: Vec::new(),
                after: clips,
                label: "Captions",
            },
        ];
        self.run(
            Command::Batch {
                label: "Captions",
                commands,
            },
            None,
        )?;
        Ok((track_id, ids))
    }

    /// Add one caption at `start` (on the captions track, made if needed), moved later if something is in the way.
    pub fn add_caption(
        &mut self,
        start: Micros,
        duration: Micros,
        text: &str,
        look: &CaptionLook,
    ) -> Result<ClipId, DocError> {
        let rate = self.project.frame_rate;
        let canvas_h = self.project.canvas.height;
        let line = CaptionLine {
            start,
            end: start + duration,
            text: text.to_owned(),
        };
        let clip = caption_clip(&line, look, rate, canvas_h);
        let id = clip.id;
        match self.caption_track() {
            Some(track) => {
                self.place_free(track, clip, start)?;
            }
            None => {
                let mut track = Track::new(TrackKind::Text, "Captions");
                track.captions = true;
                let track_id = track.id;
                let tl = self.project.timeline();
                let commands = vec![
                    Command::AddTrack {
                        track: Box::new(track),
                        index: tl.tracks.len(),
                    },
                    Command::EditTrack {
                        track: track_id,
                        before: Vec::new(),
                        after: vec![clip],
                        label: "Add caption",
                    },
                ];
                self.run(
                    Command::Batch {
                        label: "Add caption",
                        commands,
                    },
                    None,
                )?;
            }
        }
        Ok(id)
    }

    /// Give every caption on the track the same look (their words stay). One undo step.
    pub fn restyle_captions(
        &mut self,
        track: TrackId,
        look: &CaptionLook,
    ) -> Result<usize, DocError> {
        let ids: Vec<ClipId> = self
            .caption_lines(track)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let canvas_h = self.project.canvas.height;
        self.update_clips(&ids, "Caption style", |c| {
            if let Content::Text(t) = &mut c.content {
                t.style = look.style.clone();
                t.words = with_keywords(&look.words, &t.text);
                t.animators = look.animators.clone();
                t.spans.clear();
            }
            c.transform.y = (look.y * canvas_h as f32).into();
        })?;
        Ok(ids.len())
    }

    /// Join a caption with the one after it (one caption from the start of the first to the end of the second).
    pub fn merge_caption_with_next(&mut self, clip: ClipId) -> Result<ClipId, DocError> {
        let track = self.track_of(clip).ok_or(EditError::ClipNotFound(clip))?;
        let lines = self.caption_lines(track);
        let i = lines
            .iter()
            .position(|(id, _)| *id == clip)
            .ok_or(EditError::ClipNotFound(clip))?;
        let Some((next_id, next)) = lines.get(i + 1).cloned() else {
            return Err(EditError::OutOfRange.into());
        };
        let first = self
            .project
            .timeline()
            .clip(clip)
            .ok_or(EditError::ClipNotFound(clip))?
            .clone();
        let Content::Text(a) = &first.content else {
            return Err(EditError::WrongTrack.into());
        };
        let mut merged = first.clone();
        if let Content::Text(t) = &mut merged.content {
            t.text = format!("{} {}", a.text.trim_end(), next.text.trim_start());
            t.spans.clear();
            t.word_times.clear();
        }
        merged.duration = next.end - first.start;
        let tl = self.project.timeline();
        let t = tl.track(track).ok_or(EditError::TrackNotFound(track))?;
        let after: Vec<Clip> = t
            .clips
            .iter()
            .filter(|c| c.id != next_id)
            .map(|c| {
                if c.id == clip {
                    merged.clone()
                } else {
                    c.clone()
                }
            })
            .collect();
        let command = Command::EditTrack {
            track,
            before: t.clips.clone(),
            after,
            label: "Merge captions",
        };
        self.run(command, None)?;
        Ok(clip)
    }

    /// Cut a caption in two after `chars` characters. The time is shared by the length of the two parts, or the cut is
    /// made at `at` if given. Returns the new second caption.
    pub fn split_caption(
        &mut self,
        clip: ClipId,
        chars: usize,
        at: Option<Micros>,
    ) -> Result<ClipId, DocError> {
        let rate = self.project.frame_rate;
        let track = self.track_of(clip).ok_or(EditError::ClipNotFound(clip))?;
        let original = self
            .project
            .timeline()
            .clip(clip)
            .ok_or(EditError::ClipNotFound(clip))?
            .clone();
        let Content::Text(tc) = &original.content else {
            return Err(EditError::WrongTrack.into());
        };
        let all: Vec<char> = tc.text.chars().collect();
        let chars = chars.min(all.len());
        let (a, b): (String, String) = (
            all[..chars]
                .iter()
                .collect::<String>()
                .trim_end()
                .to_owned(),
            all[chars..]
                .iter()
                .collect::<String>()
                .trim_start()
                .to_owned(),
        );
        if a.is_empty() || b.is_empty() {
            return Err(EditError::OutOfRange.into());
        }
        let total = (a.chars().count() + b.chars().count()).max(1) as f64;
        let cut = at.unwrap_or_else(|| {
            original.start + (original.duration as f64 * a.chars().count() as f64 / total) as Micros
        });
        let cut = rate.snap(cut);
        if cut <= original.start || cut >= original.end() {
            return Err(EditError::OutOfRange.into());
        }
        let mut first = original.clone();
        let mut second = original.clone();
        second.id = ClipId::new();
        second.start = cut;
        second.duration = original.end() - cut;
        first.duration = cut - original.start;
        if let Content::Text(t) = &mut first.content {
            t.text = a;
            t.spans.clear();
            t.word_times.clear();
        }
        if let Content::Text(t) = &mut second.content {
            t.text = b;
            t.spans.clear();
            t.word_times.clear();
        }
        let new_id = second.id;
        let tl = self.project.timeline();
        let t = tl.track(track).ok_or(EditError::TrackNotFound(track))?;
        let mut after: Vec<Clip> = t
            .clips
            .iter()
            .map(|c| {
                if c.id == clip {
                    first.clone()
                } else {
                    c.clone()
                }
            })
            .collect();
        after.push(second);
        after.sort_by_key(|c| c.start);
        let command = Command::EditTrack {
            track,
            before: t.clips.clone(),
            after,
            label: "Split caption",
        };
        self.run(command, None)?;
        Ok(new_id)
    }

    /// Move every caption of the track later (or earlier, if negative) by `delta`. One undo step.
    pub fn shift_captions(&mut self, track: TrackId, delta: Micros) -> Result<(), DocError> {
        let rate = self.project.frame_rate;
        let t = self
            .project
            .timeline()
            .track(track)
            .ok_or(EditError::TrackNotFound(track))?;
        let first = t.clips.iter().map(|c| c.start).min().unwrap_or(0);
        let delta = rate.snap(delta.max(-first));
        let after: Vec<Clip> = t
            .clips
            .iter()
            .map(|c| Clip {
                start: c.start + delta,
                ..c.clone()
            })
            .collect();
        let command = Command::EditTrack {
            track,
            before: t.clips.clone(),
            after,
            label: "Shift captions",
        };
        self.run(command, None)
    }

    /// Replace text in all captions of the track. Returns how many captions changed. One undo step.
    pub fn replace_in_captions(
        &mut self,
        track: TrackId,
        find: &str,
        replace: &str,
        match_case: bool,
    ) -> Result<usize, DocError> {
        if find.is_empty() {
            return Ok(0);
        }
        let ids: Vec<ClipId> = self
            .caption_lines(track)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let mut changed = 0;
        self.update_clips(&ids, "Replace in captions", |c| {
            if let Content::Text(t) = &mut c.content {
                let new = if match_case {
                    t.text.replace(find, replace)
                } else {
                    replace_ignoring_case(&t.text, find, replace)
                };
                if new != t.text {
                    t.text = new;
                    t.spans.clear();
                    changed += 1;
                }
            }
        })?;
        Ok(changed)
    }
}

fn replace_ignoring_case(text: &str, find: &str, replace: &str) -> String {
    let lower_text: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    let lower_find: Vec<char> = find.chars().flat_map(char::to_lowercase).collect();
    // only safe to map positions when lowercasing kept the length (true for nearly all text)
    if lower_text.len() != text.chars().count() {
        return text.replace(find, replace);
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if i + lower_find.len() <= chars.len()
            && lower_text[i..i + lower_find.len()] == lower_find[..]
        {
            out.push_str(replace);
            i += lower_find.len();
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Project;
    use crate::time::US_PER_SEC;

    fn lines() -> Vec<CaptionLine> {
        vec![
            CaptionLine {
                start: 0,
                end: 2 * US_PER_SEC,
                text: "Hello there".into(),
            },
            CaptionLine {
                start: 2 * US_PER_SEC,
                end: 4 * US_PER_SEC,
                text: "ශ්‍රී ලංකාව".into(),
            },
            CaptionLine {
                start: 5 * US_PER_SEC,
                end: 6 * US_PER_SEC,
                text: "Goodbye Hello".into(),
            },
        ]
    }

    fn doc_with_captions() -> (Document, TrackId) {
        let mut doc = Document::new(Project::new("c"));
        let (track, _) = doc.set_captions(&lines(), &CaptionLook::default()).unwrap();
        (doc, track)
    }

    #[test]
    fn captions_become_text_clips_on_a_captions_track_in_one_undo_step() {
        let (mut doc, track) = doc_with_captions();
        let t = doc.project.timeline().track(track).unwrap();
        assert!(t.captions && t.kind == TrackKind::Text);
        assert_eq!(t.clips.len(), 3);
        assert_eq!(doc.caption_track(), Some(track));
        let listed = doc.caption_lines(track);
        assert_eq!(listed[1].1.text, "ශ්‍රී ලංකාව");
        assert_eq!(listed[1].1.start, 2 * US_PER_SEC);
        // the clips sit low on the picture
        let y = t.clips[0].transform.y.value_at(0);
        assert!((y - 0.3 * 1080.0).abs() < 1.0, "{y}");
        assert!(doc.undo());
        assert!(doc.caption_track().is_none());
    }

    #[test]
    fn making_captions_again_replaces_the_old_ones_and_trims_overlaps() {
        let (mut doc, track) = doc_with_captions();
        let overlapping = vec![
            CaptionLine {
                start: 0,
                end: 3 * US_PER_SEC,
                text: "A".into(),
            },
            CaptionLine {
                start: 2 * US_PER_SEC,
                end: 4 * US_PER_SEC,
                text: "B".into(),
            },
        ];
        let (again, ids) = doc
            .set_captions(&overlapping, &CaptionLook::default())
            .unwrap();
        assert_eq!(again, track, "the same track is used");
        assert_eq!(ids.len(), 2);
        let clips = &doc.project.timeline().track(track).unwrap().clips;
        assert_eq!(clips.len(), 2);
        assert_eq!(clips[0].end(), clips[1].start, "no overlap");
    }

    #[test]
    fn adding_one_caption_by_hand_works_with_and_without_a_track() {
        let mut doc = Document::new(Project::new("c"));
        let id = doc
            .add_caption(US_PER_SEC, 2 * US_PER_SEC, "First", &CaptionLook::default())
            .unwrap();
        let track = doc.caption_track().unwrap();
        assert!(doc.project.timeline().track(track).unwrap().captions);
        let second = doc
            .add_caption(
                2 * US_PER_SEC,
                US_PER_SEC,
                "Second",
                &CaptionLook::default(),
            )
            .unwrap();
        // it was moved later so it does not overlap the first
        let c = doc.project.timeline().clip(second).unwrap();
        assert_eq!(c.start, 3 * US_PER_SEC);
        assert!(doc.project.timeline().clip(id).is_some());
    }

    #[test]
    fn restyling_changes_every_caption_but_not_the_words() {
        let (mut doc, track) = doc_with_captions();
        let mut look = CaptionLook::default();
        look.style.color = [255, 0, 0, 255];
        look.words = WordMode::Karaoke {
            highlight: [0, 255, 0, 255],
            scale: 1.2,
        };
        look.y = -0.3;
        assert_eq!(doc.restyle_captions(track, &look).unwrap(), 3);
        for (id, line) in doc.caption_lines(track) {
            let c = doc.project.timeline().clip(id).unwrap();
            let Content::Text(t) = &c.content else {
                panic!()
            };
            assert_eq!(t.style.color, [255, 0, 0, 255]);
            assert!(matches!(t.words, WordMode::Karaoke { .. }));
            assert_eq!(t.text, line.text);
            assert!(c.transform.y.value_at(0) < 0.0);
        }
        assert!(doc.undo(), "one undo step for all of them");
        let first = doc.caption_lines(track)[0].0;
        let Content::Text(t) = &doc.project.timeline().clip(first).unwrap().content else {
            panic!()
        };
        assert_eq!(t.style.color, [255, 255, 255, 255]);
    }

    #[test]
    fn two_captions_can_be_merged_and_one_can_be_cut_in_two() {
        let (mut doc, track) = doc_with_captions();
        let (first, second) = {
            let l = doc.caption_lines(track);
            (l[0].0, l[1].0)
        };
        doc.merge_caption_with_next(first).unwrap();
        let l = doc.caption_lines(track);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].1.text, "Hello there ශ්‍රී ලංකාව");
        assert_eq!((l[0].1.start, l[0].1.end), (0, 4 * US_PER_SEC));
        assert!(doc.project.timeline().clip(second).is_none());
        // cut it again after "Hello there"
        let new = doc.split_caption(first, 11, None).unwrap();
        let l = doc.caption_lines(track);
        assert_eq!(l.len(), 3);
        assert_eq!(l[0].1.text, "Hello there");
        assert_eq!(l[1].1.text, "ශ්‍රී ලංකාව");
        assert_eq!(l[1].0, new);
        assert_eq!(l[0].1.end, l[1].1.start);
        // cutting at an exact time, and refusing silly cuts
        assert!(doc.split_caption(new, 0, None).is_err());
        let third = doc.split_caption(first, 5, Some(US_PER_SEC)).unwrap();
        assert_eq!(
            doc.project.timeline().clip(third).unwrap().start,
            US_PER_SEC
        );
        assert!(
            doc.merge_caption_with_next(doc.caption_lines(track).last().unwrap().0)
                .is_err(),
            "the last has no next"
        );
    }

    #[test]
    fn all_timings_can_be_shifted_but_never_before_zero() {
        let (mut doc, track) = doc_with_captions();
        doc.shift_captions(track, 3 * US_PER_SEC).unwrap();
        assert_eq!(doc.caption_lines(track)[0].1.start, 3 * US_PER_SEC);
        doc.shift_captions(track, -99 * US_PER_SEC).unwrap();
        assert_eq!(doc.caption_lines(track)[0].1.start, 0, "stopped at zero");
        assert_eq!(
            doc.caption_lines(track)[1].1.start,
            2 * US_PER_SEC,
            "spacing is kept"
        );
    }

    #[test]
    fn find_and_replace_works_across_all_captions_with_and_without_case() {
        let (mut doc, track) = doc_with_captions();
        assert_eq!(
            doc.replace_in_captions(track, "hello", "Bye", false)
                .unwrap(),
            2
        );
        let texts: Vec<_> = doc
            .caption_lines(track)
            .into_iter()
            .map(|(_, l)| l.text)
            .collect();
        assert_eq!(texts, ["Bye there", "ශ්‍රී ලංකාව", "Goodbye Bye"]);
        assert_eq!(
            doc.replace_in_captions(track, "bye", "x", true).unwrap(),
            1,
            "match case: only the lower-case 'bye' in Goodbye"
        );
        assert_eq!(doc.replace_in_captions(track, "", "x", true).unwrap(), 0);
        assert_eq!(
            doc.replace_in_captions(track, "ලංකාව", "ශ්‍රී", true).unwrap(),
            1
        );
    }
}
