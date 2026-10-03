//! Playback engine, mixer, audio effects, loudness and beat detection.
//!
//! Part of vidzcut. See docs/ARCHITECTURE.md.
//! Phase 1: the timeline's sound is read from the media files, mixed and played; the sound
//! card's clock is the master clock. Effects arrive in Phase 6.

pub mod engine;
pub mod mixer;
pub mod reader;
pub mod waveform;

pub use engine::AudioEngine;
pub use mixer::{Mixer, micros_to_samples, samples_to_micros};
pub use reader::{AudioError, AudioReader};
pub use waveform::Waveform;

/// Name of this crate, used in diagnostics.
pub const CRATE: &str = "vz-audio";

#[cfg(test)]
mod tests {
    use super::*;
    use vz_core::{Document, Project, US_PER_SEC};
    use vz_media::testutil::{self, Spec};

    const RATE: u32 = 48_000;

    fn doc_with(path: &std::path::Path) -> Document {
        let mut doc = Document::new(Project::new("audio test"));
        let id = doc.import_media(vz_media::probe(path).unwrap()).unwrap();
        doc.add_clip(id, usize::MAX).unwrap();
        doc
    }

    /// Estimate the pitch of a mono-ish signal from its zero crossings.
    fn pitch_hz(stereo: &[f32]) -> f32 {
        let left: Vec<f32> = stereo.iter().step_by(2).copied().collect();
        let crossings = left
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        crossings as f32 / (left.len() as f32 / RATE as f32)
    }

    fn rms(stereo: &[f32]) -> f32 {
        (stereo.iter().map(|v| v * v).sum::<f32>() / stereo.len().max(1) as f32).sqrt()
    }

    #[test]
    fn sample_conversion() {
        assert_eq!(micros_to_samples(US_PER_SEC, RATE), 48_000);
        assert_eq!(micros_to_samples(0, RATE), 0);
        assert_eq!(samples_to_micros(48_000, RATE), US_PER_SEC);
        assert_eq!(
            micros_to_samples(samples_to_micros(12_345, RATE), RATE),
            12_345
        );
    }

    #[test]
    fn the_reader_gives_the_tone_that_is_in_the_file() {
        let path = testutil::standard(); // 440 Hz
        let mut r = AudioReader::open(&path, RATE).unwrap();
        let mut out = vec![0.0; 2 * RATE as usize]; // 1 s
        r.read(0, &mut out).unwrap();
        let hz = pitch_hz(&out[2000..]);
        assert!((hz - 440.0).abs() < 8.0, "pitch was {hz}");
        assert!(rms(&out[4000..]) > 0.05, "should not be silent");
    }

    #[test]
    fn reading_in_pieces_equals_reading_at_once() {
        let path = testutil::standard();
        let mut a = AudioReader::open(&path, RATE).unwrap();
        let mut whole = vec![0.0; 2 * 24_000];
        a.read(10_000, &mut whole).unwrap();

        let mut b = AudioReader::open(&path, RATE).unwrap();
        let mut pieces = Vec::new();
        for i in 0..6 {
            let mut p = vec![0.0; 2 * 4_000];
            b.read(10_000 + i * 4_000, &mut p).unwrap();
            pieces.extend(p);
        }
        assert_eq!(whole, pieces);
    }

    #[test]
    fn seeking_backwards_and_far_ahead_gives_the_same_sound() {
        let path = testutil::standard();
        let mut r = AudioReader::open(&path, RATE).unwrap();
        let mut first = vec![0.0; 2 * 4_800];
        r.read(96_000, &mut first).unwrap(); // 2 s in
        let mut elsewhere = vec![0.0; 2 * 4_800];
        r.read(1_000, &mut elsewhere).unwrap(); // back near the start
        let mut again = vec![0.0; 2 * 4_800];
        r.read(96_000, &mut again).unwrap(); // and forward again
        let diff = first
            .iter()
            .zip(&again)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(diff < 0.02, "sound differs after seeking by {diff}");
    }

    #[test]
    fn the_end_of_the_file_is_silence() {
        let path = testutil::standard(); // 4 s
        let mut r = AudioReader::open(&path, RATE).unwrap();
        let mut out = vec![1.0; 2 * 4_800];
        r.read(5 * i64::from(RATE), &mut out).unwrap();
        assert!(out.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn a_video_without_sound_is_refused_by_the_reader_and_silent_in_the_mixer() {
        let silent = testutil::video(
            "silent_320x240.mp4",
            Spec {
                width: 320,
                height: 240,
                audio_hz: None,
                seconds: 2,
                ..Spec::default()
            },
        );
        assert!(matches!(
            AudioReader::open(&silent, RATE),
            Err(AudioError::NoAudio(_))
        ));
        let doc = doc_with(&silent);
        let mut m = Mixer::new(RATE);
        let mut out = Vec::new();
        m.render(&doc.project, 0, 4_800, &mut out);
        assert!(out.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn the_mixer_plays_clips_where_they_are_and_silence_elsewhere() {
        let path = testutil::standard();
        let doc = doc_with(&path); // clip 0..4 s
        let mut m = Mixer::new(RATE);
        let mut out = Vec::new();
        m.render(&doc.project, 0, RATE as usize, &mut out);
        assert!(rms(&out[4000..]) > 0.05);
        // after the clip ends: nothing
        m.render(&doc.project, 6 * i64::from(RATE), 4_800, &mut out);
        assert!(out.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn a_cut_does_not_change_the_sound() {
        let path = testutil::standard();
        let mut whole = doc_with(&path);
        let mut cut = doc_with(&path);
        cut.split_at(2 * US_PER_SEC).unwrap();
        let mut m1 = Mixer::new(RATE);
        let mut m2 = Mixer::new(RATE);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        // a stretch that crosses the cut at 2 s
        let start = 2 * i64::from(RATE) - 4_800;
        m1.render(&whole.project, start, 9_600, &mut a);
        m2.render(&cut.project, start, 9_600, &mut b);
        let diff = a
            .iter()
            .zip(&b)
            .skip(100)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f32::max);
        assert!(
            rms(&a) > 0.05 && rms(&b) > 0.05,
            "there must be sound to compare"
        );
        assert!(diff < 0.05, "the cut changed the sound by {diff}");
        whole.undo();
    }

    #[test]
    fn a_trimmed_clip_plays_the_right_part_of_the_file() {
        let path = testutil::standard();
        let mut doc = doc_with(&path);
        let id = doc.project.timeline().main_track().clips[0].id;
        doc.trim_left(id, 2 * US_PER_SEC, None).unwrap();
        // timeline 0 now shows file time 2 s
        let mut trimmed = Vec::new();
        Mixer::new(RATE).render(&doc.project, 0, 4_800, &mut trimmed);
        let mut reference = vec![0.0; 2 * 4_800];
        AudioReader::open(&path, RATE)
            .unwrap()
            .read(2 * i64::from(RATE), &mut reference)
            .unwrap();
        let diff = trimmed
            .iter()
            .zip(&reference)
            .skip(100)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f32::max);
        assert!(rms(&trimmed) > 0.05, "there must be sound to compare");
        assert!(diff < 0.05, "trimmed clip differs from the file by {diff}");
    }

    #[test]
    fn the_sound_card_clock_runs_in_real_time() {
        // plays SILENCE through the real sound device; skipped on machines without one (like CI)
        let Ok(engine) = AudioEngine::start() else {
            eprintln!("no sound device here, skipping");
            return;
        };
        let silent = testutil::video(
            "silent_320x240.mp4",
            Spec {
                width: 320,
                height: 240,
                audio_hz: None,
                seconds: 2,
                ..Spec::default()
            },
        );
        let doc = doc_with(&silent);
        engine.set_project(std::sync::Arc::new(doc.project.clone()));
        engine.play(US_PER_SEC / 2); // start half a second in
        std::thread::sleep(std::time::Duration::from_millis(1000));
        let t = engine.position();
        engine.pause();
        let played = t - US_PER_SEC / 2;
        assert!(
            (500_000..1_300_000).contains(&played),
            "after 1 s the clock moved by {played} us"
        );
        assert!(!engine.is_playing());
    }

    /// RMS of the mixed sound between two times.
    fn loudness(doc: &Document, from: i64, to: i64) -> f32 {
        let mut mixer = Mixer::new(RATE);
        let start = micros_to_samples(from, RATE);
        let frames = (micros_to_samples(to, RATE) - start) as usize;
        let mut out = Vec::new();
        mixer.render(&doc.project, start, frames, &mut out);
        rms(&out)
    }

    #[test]
    fn volume_scales_the_sound() {
        let path = testutil::standard();
        let mut doc = doc_with(&path);
        let id = doc.project.timeline().main_track().clips[0].id;
        let full = loudness(&doc, 0, US_PER_SEC);
        doc.update_clip(id, "v", None, |c| c.volume = 0.5.into())
            .unwrap();
        let half = loudness(&doc, 0, US_PER_SEC);
        assert!((half / full - 0.5).abs() < 0.02, "{half} vs {full}");
        doc.update_clip(id, "v", None, |c| c.volume = 0.0.into())
            .unwrap();
        assert_eq!(loudness(&doc, 0, US_PER_SEC), 0.0);
    }

    #[test]
    fn fades_ramp_the_sound_in_and_out() {
        let path = testutil::standard(); // 4 s
        let mut doc = doc_with(&path);
        let id = doc.project.timeline().main_track().clips[0].id;
        let plain_start = loudness(&doc, 0, US_PER_SEC / 4);
        doc.update_clip(id, "f", None, |c| {
            c.fade_in = US_PER_SEC;
            c.fade_out = US_PER_SEC;
        })
        .unwrap();
        // the first quarter second of a one second fade-in is quiet, and the middle is untouched
        assert!(loudness(&doc, 0, US_PER_SEC / 4) < plain_start * 0.3);
        let middle = loudness(&doc, US_PER_SEC * 3 / 2, US_PER_SEC * 5 / 2);
        assert!((middle - plain_start).abs() < plain_start * 0.1);
        // the last quarter second fades out
        assert!(
            loudness(&doc, 4 * US_PER_SEC - US_PER_SEC / 4, 4 * US_PER_SEC) < plain_start * 0.3
        );
        // and the very first sample is silent
        let mut mixer = Mixer::new(RATE);
        let mut out = Vec::new();
        mixer.render(&doc.project, 0, 8, &mut out);
        assert_eq!(out[0], 0.0);
    }

    #[test]
    fn a_volume_ramp_between_keyframes_is_followed() {
        use vz_anim::{Easing, Keyframe};
        let path = testutil::standard();
        let mut doc = doc_with(&path);
        let id = doc.project.timeline().main_track().clips[0].id;
        doc.update_clip(id, "v", None, |c| {
            c.volume = vz_anim::Animatable::Keyframed(vec![
                Keyframe {
                    time: 0,
                    value: 1.0,
                    easing: Easing::Linear,
                },
                Keyframe {
                    time: 2 * US_PER_SEC,
                    value: 0.0,
                    easing: Easing::Linear,
                },
            ]);
        })
        .unwrap();
        let early = loudness(&doc, 0, US_PER_SEC / 4);
        let late = loudness(&doc, US_PER_SEC * 7 / 4, 2 * US_PER_SEC);
        assert!(late < early * 0.25, "{late} vs {early}");
    }

    #[test]
    fn a_muted_track_is_silent_and_a_video_overlay_adds_its_sound() {
        let path = testutil::standard();
        let mut doc = doc_with(&path);
        let main = doc.project.timeline().main_track().id;
        let sound = loudness(&doc, 0, US_PER_SEC);
        assert!(sound > 0.01);
        doc.set_track_flags(main, |f| f.muted = true).unwrap();
        assert_eq!(loudness(&doc, 0, US_PER_SEC), 0.0);
        doc.set_track_flags(main, |f| f.muted = false).unwrap();
        let media = doc.project.media[0].id;
        doc.place_media_new_track(media, 0).unwrap(); // the same sound again on an audio track
        let doubled = loudness(&doc, 0, US_PER_SEC);
        assert!(doubled > sound * 1.5, "{doubled} vs {sound}");
    }
}
