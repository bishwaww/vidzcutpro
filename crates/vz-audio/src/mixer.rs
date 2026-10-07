//! Mixes everything on the timeline into one stereo stream. Playback and export both use this,
//! so what you hear is what you export.

use crate::fx::Chain;
use crate::reader::AudioReader;
use crate::retime::Retimer;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use vz_core::{
    AudioFx, ChannelMode, Clip, ClipId, Content, Duck, MediaKind, Micros, Project, Timeline,
};

/// Microseconds to a sample number at `rate` (rounded to the nearest sample).
pub fn micros_to_samples(t: Micros, rate: u32) -> i64 {
    ((i128::from(t) * i128::from(rate) + 500_000).div_euclid(1_000_000)) as i64
}

/// A sample number back to microseconds.
pub fn samples_to_micros(samples: i64, rate: u32) -> Micros {
    (i128::from(samples) * 1_000_000 / i128::from(rate.max(1))) as Micros
}

/// What the mixer remembers about one clip between chunks.
struct ClipState {
    /// The settings the chain was built from (a change means a new chain).
    fx: AudioFx,
    chain: Option<Chain>,
    /// Clip sample the chain expects next; anything else is a jump (a seek) and starts the chain over.
    next: i64,
    /// Ducking: how loud the sound that triggers it is, and how far down this clip is (0 to 1).
    trigger: f32,
    ducked: f32,
}

impl ClipState {
    fn new() -> Self {
        Self {
            fx: AudioFx::default(),
            chain: None,
            next: i64::MIN,
            trigger: 0.0,
            ducked: 0.0,
        }
    }
}

pub struct Mixer {
    rate: u32,
    readers: HashMap<PathBuf, AudioReader>,
    /// Sound makers for clips whose speed is not plain (one per clip, they remember where they were).
    retimers: HashMap<ClipId, Retimer>,
    /// Effect chains and ducking state, one per clip.
    states: HashMap<ClipId, ClipState>,
    /// Files whose sound could not be read; logged once, then treated as silent.
    broken: HashSet<PathBuf>,
    scratch: Vec<f32>,
}

impl Mixer {
    pub fn new(rate: u32) -> Self {
        Self {
            rate,
            readers: HashMap::new(),
            retimers: HashMap::new(),
            states: HashMap::new(),
            broken: HashSet::new(),
            scratch: Vec::new(),
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Sound for timeline samples `start_sample .. start_sample + frames`, as interleaved stereo in `out`
    /// (`out` is resized to `frames * 2`).
    pub fn render(
        &mut self,
        project: &Project,
        start_sample: i64,
        frames: usize,
        out: &mut Vec<f32>,
    ) {
        out.clear();
        out.resize(frames * 2, 0.0);
        if self.retimers.len() > 16 || self.states.len() > 32 {
            let alive: HashSet<_> = project
                .timelines
                .iter()
                .flat_map(|t| t.tracks.iter())
                .flat_map(|t| t.clips.iter().map(|c| c.id))
                .collect();
            self.retimers.retain(|id, _| alive.contains(id));
            self.states.retain(|id, _| alive.contains(id));
        }
        self.mix(project, project.timeline(), start_sample, frames, out, 0);
        for v in out.iter_mut() {
            *v = soft_limit(*v);
        }
    }

    /// Add the sound of `timeline` for samples `start_sample .. start_sample + frames` into `out` (interleaved stereo).
    /// Compound clips play their own timeline (at normal speed; a retimed compound is silent for now).
    fn mix(
        &mut self,
        project: &Project,
        timeline: &Timeline,
        start_sample: i64,
        frames: usize,
        out: &mut [f32],
        depth: u8,
    ) {
        let end_sample = start_sample + frames as i64;
        let any_ducked = timeline
            .tracks
            .iter()
            .any(|t| !t.muted && t.clips.iter().any(|c| c.audio.duck.is_some()));
        // pass 0: everything that plays normally; pass 1: clips that duck under what pass 0 made
        let mut trigger: Vec<f32> = Vec::new();
        let mut buf = std::mem::take(&mut self.scratch);
        for pass in 0..if any_ducked { 2 } else { 1 } {
            if pass == 1 {
                trigger = out.to_vec();
            }
            for track in &timeline.tracks {
                if track.muted {
                    continue;
                }
                for clip in &track.clips {
                    if clip.audio.duck.is_some() != (pass == 1) {
                        continue;
                    }
                    let clip_start = micros_to_samples(clip.start, self.rate);
                    let clip_end = micros_to_samples(clip.end(), self.rate);
                    let from = start_sample.max(clip_start);
                    let to = end_sample.min(clip_end);
                    if from >= to {
                        continue;
                    }
                    let n = (to - from) as usize;
                    if !self.clip_sound(project, clip, from, clip_start, n, depth, &mut buf) {
                        continue;
                    }
                    let offset = (from - start_sample) as usize;
                    if pass == 1
                        && let Some(duck) = clip.audio.duck
                    {
                        self.apply_duck(clip.id, &duck, &trigger[offset * 2..][..n * 2], &mut buf);
                    }
                    let dst = &mut out[offset * 2..][..n * 2];
                    for (d, s) in dst.iter_mut().zip(&buf) {
                        *d += s;
                    }
                }
            }
        }
        self.scratch = buf;
    }

    /// The sound of `clip` for timeline samples `from .. from + n`, with effects, volume and fades, in `buf`.
    /// False if the clip makes no sound there.
    #[allow(clippy::too_many_arguments)]
    fn clip_sound(
        &mut self,
        project: &Project,
        clip: &Clip,
        from: i64,
        clip_start: i64,
        n: usize,
        depth: u8,
        buf: &mut Vec<f32>,
    ) -> bool {
        buf.clear();
        buf.resize(n * 2, 0.0);
        let first = from - clip_start;
        if let Content::Compound(inner) = &clip.content {
            let Some(inner) = project.timeline_by_id(*inner) else {
                return false;
            };
            if clip.is_retimed() || depth >= 6 {
                return false;
            }
            let inner_start = micros_to_samples(clip.source_in, self.rate) + first;
            self.mix(project, inner, inner_start, n, buf, depth + 1);
        } else {
            let Some(asset) = clip.media().and_then(|m| project.asset(m)) else {
                return false;
            };
            if !asset.has_audio
                || asset.kind == MediaKind::Image
                || self.broken.contains(&asset.path)
            {
                return false;
            }
            if !self.readers.contains_key(&asset.path) {
                match AudioReader::open(&asset.path, self.rate) {
                    Ok(r) => {
                        self.readers.insert(asset.path.clone(), r);
                    }
                    Err(e) => {
                        tracing::warn!("no sound from {}: {e}", asset.path.display());
                        self.broken.insert(asset.path.clone());
                        return false;
                    }
                }
            }
            let reader = self.readers.get_mut(&asset.path).expect("opened above");
            let result = if clip.is_retimed() {
                self.retimers
                    .entry(clip.id)
                    .or_default()
                    .render(reader, clip, self.rate, first, buf)
            } else {
                let file_start = micros_to_samples(clip.source_in, self.rate) + first;
                reader.read(file_start, buf)
            };
            if let Err(e) = result {
                tracing::warn!("sound read error in {}: {e}", asset.path.display());
                self.readers.remove(&asset.path);
                self.broken.insert(asset.path.clone());
                return false;
            }
        }
        if clip.audio.needs_chain() {
            let state = self.states.entry(clip.id).or_insert_with(ClipState::new);
            if state.chain.is_none() || state.fx != clip.audio || state.next != first {
                state.fx = clip.audio.clone();
                state.chain = Some(Chain::new(self.rate, &clip.audio));
            }
            if let Some(chain) = &mut state.chain {
                chain.process(buf);
            }
            state.next = first + n as i64;
        } else if let Some(state) = self.states.get_mut(&clip.id) {
            state.chain = None;
        }
        apply_gain(clip, self.rate, first, buf);
        true
    }

    /// Lower `buf` while `trigger` (the sound of everything that does not duck) is loud.
    fn apply_duck(&mut self, id: ClipId, duck: &Duck, trigger: &[f32], buf: &mut [f32]) {
        let rate = self.rate as f32;
        let coef = |ms: f32| (-1.0 / (ms.max(0.1) * 0.001 * rate)).exp();
        let (listen_up, listen_down) = (coef(4.0), coef(80.0));
        let (down, up) = (coef(duck.attack_ms), coef(duck.release_ms));
        let state = self.states.entry(id).or_insert_with(ClipState::new);
        let depth_db = duck.amount_db.clamp(0.0, 60.0);
        for (frame, heard) in buf
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(trigger.as_chunks::<2>().0)
        {
            let level = heard[0].abs().max(heard[1].abs());
            let c = if level > state.trigger {
                listen_up
            } else {
                listen_down
            };
            state.trigger = c * state.trigger + (1.0 - c) * level;
            // quiet sound below -40 dB does not duck; clearly audible sound ducks fully
            let want = ((state.trigger - 0.01) / 0.03).clamp(0.0, 1.0);
            let c = if want > state.ducked { down } else { up };
            state.ducked = c * state.ducked + (1.0 - c) * want;
            let gain = 10f32.powf(-depth_db * state.ducked / 20.0);
            frame[0] *= gain;
            frame[1] *= gain;
        }
    }
}

/// Rounds off the very top of the mix (above 0.9) so loud sums are not harshly clipped.
fn soft_limit(v: f32) -> f32 {
    const KNEE: f32 = 0.9;
    let a = v.abs();
    if a <= KNEE {
        v
    } else {
        v.signum() * (KNEE + (1.0 - KNEE) * ((a - KNEE) / (1.0 - KNEE)).tanh())
    }
}

/// Apply the clip's channel choice, balance, loudness gain, volume and fades to `samples`
/// (interleaved stereo) that start `first` samples into the clip.
fn apply_gain(clip: &Clip, rate: u32, first: i64, samples: &mut [f32]) {
    let length = micros_to_samples(clip.duration, rate);
    let fade_in = micros_to_samples(clip.fade_in.clamp(0, clip.duration), rate);
    let fade_out = micros_to_samples(clip.fade_out.clamp(0, clip.duration), rate);
    let animated = clip.volume.is_animated();
    let mut volume = clip
        .volume
        .value_at(samples_to_micros(first, rate))
        .max(0.0);
    let fixed = clip.audio.normalize_factor();
    let pan = clip.audio.pan.clamp(-1.0, 1.0);
    let (pan_l, pan_r) = ((1.0 - pan).min(1.0), (1.0 + pan).min(1.0));
    let channel = clip.audio.channel;
    for (i, frame) in samples.as_chunks_mut::<2>().0.iter_mut().enumerate() {
        let at = first + i as i64;
        if animated && i % 256 == 0 {
            volume = clip.volume.value_at(samples_to_micros(at, rate)).max(0.0);
        }
        let mut gain = volume * fixed;
        if fade_in > 0 && at < fade_in {
            gain *= at as f32 / fade_in as f32;
        }
        let left = length - at;
        if fade_out > 0 && left < fade_out {
            gain *= left.max(0) as f32 / fade_out as f32;
        }
        match channel {
            ChannelMode::Stereo => {}
            ChannelMode::Left => frame[1] = frame[0],
            ChannelMode::Right => frame[0] = frame[1],
            ChannelMode::Mono => {
                let m = (frame[0] + frame[1]) * 0.5;
                frame[0] = m;
                frame[1] = m;
            }
        }
        frame[0] *= gain * pan_l;
        frame[1] *= gain * pan_r;
    }
}
