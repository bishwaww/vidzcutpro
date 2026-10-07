//! The sound effects as heard through the mixer: what the inspector's controls do to a real clip.

use crate::{Mixer, micros_to_samples};
use vz_core::{
    AudioFx, ChannelMode, Compressor, Document, Duck, Eq, Normalize, Project, US_PER_SEC, Voice,
    VoiceEffect,
};
use vz_media::testutil;

const RATE: u32 = 48_000;

fn doc_with(path: &std::path::Path) -> Document {
    let mut doc = Document::new(Project::new("fx test"));
    let id = doc.import_media(vz_media::probe(path).unwrap()).unwrap();
    doc.add_clip(id, usize::MAX).unwrap();
    doc
}

fn first_clip(doc: &Document) -> vz_core::ClipId {
    doc.project.timeline().main_track().clips[0].id
}

fn mix(doc: &Document, from: i64, to: i64) -> Vec<f32> {
    let mut mixer = Mixer::new(RATE);
    let start = micros_to_samples(from, RATE);
    let frames = (micros_to_samples(to, RATE) - start) as usize;
    let mut out = Vec::new();
    // in pieces like the player does, so the effect chains keep their state between pieces
    let mut all = Vec::new();
    let mut done = 0;
    while done < frames {
        let n = (frames - done).min(1024);
        mixer.render(&doc.project, start + done as i64, n, &mut out);
        all.extend_from_slice(&out);
        done += n;
    }
    all
}

fn rms(stereo: &[f32]) -> f32 {
    (stereo.iter().map(|v| v * v).sum::<f32>() / stereo.len().max(1) as f32).sqrt()
}

fn set(doc: &mut Document, f: impl FnOnce(&mut AudioFx)) {
    let id = first_clip(doc);
    doc.update_clip(id, "fx", None, |c| f(&mut c.audio))
        .unwrap();
}

#[test]
fn normalize_applies_the_measured_difference() {
    let mut doc = doc_with(&testutil::standard());
    let plain = rms(&mix(&doc, US_PER_SEC / 2, 2 * US_PER_SEC));
    set(&mut doc, |a| {
        a.normalize = Some(Normalize {
            target_lufs: -14.0,
            measured_lufs: -20.0,
        });
    });
    let louder = rms(&mix(&doc, US_PER_SEC / 2, 2 * US_PER_SEC));
    // +6 dB is a factor of two (the soft limiter only touches the very top)
    assert!((louder / plain - 1.995).abs() < 0.06, "{louder} vs {plain}");
}

#[test]
fn balance_and_channel_choices_move_the_sound() {
    let mut doc = doc_with(&testutil::standard());
    set(&mut doc, |a| a.pan = 1.0);
    let out = mix(&doc, US_PER_SEC / 2, US_PER_SEC);
    let left: f32 = out.iter().step_by(2).map(|v| v.abs()).sum();
    let right: f32 = out.iter().skip(1).step_by(2).map(|v| v.abs()).sum();
    assert_eq!(left, 0.0, "hard right leaves the left silent");
    assert!(right > 1.0);

    set(&mut doc, |a| {
        a.pan = 0.0;
        a.channel = ChannelMode::Left;
    });
    let out = mix(&doc, US_PER_SEC / 2, US_PER_SEC);
    assert!(out.as_chunks::<2>().0.iter().all(|f| f[0] == f[1]));
}

#[test]
fn the_compressor_lowers_loud_sound_and_a_flat_equaliser_changes_nothing() {
    let mut doc = doc_with(&testutil::standard());
    let plain = mix(&doc, US_PER_SEC / 2, 2 * US_PER_SEC);
    set(&mut doc, |a| a.eq = Eq::default());
    assert_eq!(mix(&doc, US_PER_SEC / 2, 2 * US_PER_SEC), plain);
    set(&mut doc, |a| {
        a.compressor = Some(Compressor {
            threshold_db: -40.0,
            ratio: 10.0,
            attack_ms: 1.0,
            release_ms: 50.0,
            makeup_db: 0.0,
            limit: false,
        });
    });
    let squashed = mix(&doc, US_PER_SEC / 2, 2 * US_PER_SEC);
    assert!(rms(&squashed) < rms(&plain) * 0.5);
}

#[test]
fn a_bass_cut_removes_a_low_tone_and_a_voice_effect_changes_the_pitch() {
    // the test tone is 440 Hz: an equaliser cut of 12 dB at 350 Hz and 1 kHz lowers it
    let mut doc = doc_with(&testutil::standard());
    let plain = rms(&mix(&doc, US_PER_SEC / 2, 2 * US_PER_SEC));
    set(&mut doc, |a| a.eq.gains = [0.0, -12.0, -12.0, 0.0, 0.0]);
    let cut = rms(&mix(&doc, US_PER_SEC / 2, 2 * US_PER_SEC));
    assert!(cut < plain * 0.4, "{cut} vs {plain}");

    set(&mut doc, |a| {
        a.eq = Eq::default();
        a.voice = Some(Voice {
            effect: VoiceEffect::Chipmunk,
            amount: 0.5,
        });
    });
    let out = mix(&doc, US_PER_SEC, 3 * US_PER_SEC);
    let left: Vec<f32> = out.iter().step_by(2).copied().collect();
    let crossings = left
        .windows(2)
        .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
        .count();
    let hz = crossings as f32 / (left.len() as f32 / RATE as f32);
    assert!(
        hz > 700.0 && hz < 800.0,
        "440 Hz times 1.7 should be about 750 Hz, got {hz}"
    );
}

#[test]
fn ducking_lowers_a_clip_while_other_sound_plays_and_not_when_alone() {
    let path = testutil::standard();
    let mut doc = doc_with(&path);
    let media = doc.project.media[0].id;
    // the same sound again on an audio track, ducked hard
    let (_, music) = doc.place_media_new_track(media, 0).unwrap();
    doc.update_clip(music, "duck", None, |c| {
        c.audio.duck = Some(Duck {
            amount_db: 40.0,
            attack_ms: 5.0,
            release_ms: 100.0,
        });
    })
    .unwrap();
    let both = rms(&mix(&doc, US_PER_SEC, 3 * US_PER_SEC));
    let main = doc.project.timeline().main_track().id;
    // mute the main track: the ducked clip is alone, so nothing ducks it
    doc.set_track_flags(main, |f| f.muted = true).unwrap();
    let alone = rms(&mix(&doc, US_PER_SEC, 3 * US_PER_SEC));
    doc.set_track_flags(main, |f| f.muted = false).unwrap();
    doc.update_clip(music, "duck", None, |c| c.audio.duck = None)
        .unwrap();
    let doubled = rms(&mix(&doc, US_PER_SEC, 3 * US_PER_SEC));
    // ducked: about the main clip alone; not ducked: twice as loud
    assert!(both < doubled * 0.65, "ducked {both} vs {doubled}");
    assert!(alone > both * 0.6, "alone {alone} vs ducked {both}");
}

#[test]
fn old_projects_without_sound_settings_still_play_the_same() {
    let doc = doc_with(&testutil::standard());
    assert!(
        doc.project.timeline().main_track().clips[0]
            .audio
            .is_plain()
    );
    assert!(rms(&mix(&doc, 0, US_PER_SEC)) > 0.05);
}
