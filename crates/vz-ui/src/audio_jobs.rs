//! Sound measurements that run in the background: loudness, background noise and beats.
//! Results are written into the clip when they arrive (as one undo step each).

use crossbeam_channel::{Receiver, Sender};
use eframe::egui;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use vz_audio::Wanted;
use vz_core::{ClipId, Document, MediaId, Micros, Normalize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JobKind {
    Loudness,
    Noise,
    Beats,
}

struct Done {
    clip: ClipId,
    kind: JobKind,
    result: Result<vz_audio::Analysis, String>,
}

struct ExtractDone {
    media: MediaId,
    result: Result<PathBuf, String>,
}

/// Write the whole sound of a file as a 48 kHz stereo WAV.
fn extract_to_wav(
    path: &std::path::Path,
    duration: Micros,
    out: &std::path::Path,
) -> Result<(), String> {
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let rate = vz_audio::analysis::ANALYSIS_RATE;
    let mut reader = vz_audio::AudioReader::open(path, rate).map_err(|e| e.to_string())?;
    let mut wav = vz_audio::wav::WavWriter::create(out, rate, 2).map_err(|e| e.to_string())?;
    let total = vz_audio::micros_to_samples(duration, rate);
    let chunk = rate as usize;
    let mut buf = vec![0.0f32; chunk * 2];
    let mut at = 0i64;
    while at < total {
        let frames = ((total - at) as usize).min(chunk);
        reader
            .read(at, &mut buf[..frames * 2])
            .map_err(|e| e.to_string())?;
        wav.write(&buf[..frames * 2]).map_err(|e| e.to_string())?;
        at += frames as i64;
    }
    wav.finish().map_err(|e| e.to_string())
}

pub struct AudioJobs {
    extracting: HashSet<MediaId>,
    extracted: (Sender<ExtractDone>, Receiver<ExtractDone>),
    busy: HashSet<(ClipId, JobKind)>,
    sender: Sender<Done>,
    receiver: Receiver<Done>,
}

impl Default for AudioJobs {
    fn default() -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded();
        Self {
            extracting: HashSet::new(),
            extracted: crossbeam_channel::unbounded(),
            busy: HashSet::new(),
            sender,
            receiver,
        }
    }
}

impl AudioJobs {
    pub fn extracting(&self, media: MediaId) -> bool {
        self.extracting.contains(&media)
    }

    /// Save the sound of a video as its own file (then it is imported into the project).
    pub fn extract(
        &mut self,
        egui_ctx: &egui::Context,
        media: MediaId,
        path: PathBuf,
        duration: Micros,
        out: PathBuf,
    ) {
        if !self.extracting.insert(media) {
            return;
        }
        let (sender, wake) = (self.extracted.0.clone(), egui_ctx.clone());
        std::thread::spawn(move || {
            let result = extract_to_wav(&path, duration, &out).map(|()| out);
            let _ = sender.send(ExtractDone { media, result });
            wake.request_repaint();
        });
    }

    pub fn busy(&self, clip: ClipId, kind: JobKind) -> bool {
        self.busy.contains(&(clip, kind))
    }

    /// Measure `length` of the file from `from`. Does nothing if the same job is already running.
    pub fn start(
        &mut self,
        egui_ctx: &egui::Context,
        clip: ClipId,
        kind: JobKind,
        path: PathBuf,
        from: Micros,
        length: Micros,
    ) {
        if !self.busy.insert((clip, kind)) {
            return;
        }
        let wanted = Wanted {
            loudness: kind == JobKind::Loudness,
            noise: kind == JobKind::Noise,
            beats: kind == JobKind::Beats,
        };
        let (sender, wake) = (self.sender.clone(), egui_ctx.clone());
        std::thread::spawn(move || {
            let never = AtomicBool::new(false);
            let result = match vz_audio::analyse(&path, from, length, wanted, &never) {
                Ok(Some(a)) => Ok(a),
                Ok(None) => Err("cancelled".to_string()),
                Err(e) => Err(e.to_string()),
            };
            let _ = sender.send(Done { clip, kind, result });
            wake.request_repaint();
        });
    }

    /// Put finished measurements into their clips. Returns messages to show the user.
    pub fn poll(&mut self, doc: &mut Document) -> Vec<String> {
        let mut messages = Vec::new();
        while let Ok(done) = self.extracted.1.try_recv() {
            self.extracting.remove(&done.media);
            match done.result {
                Ok(path) => match vz_media::probe(&path) {
                    Ok(asset) => {
                        let name = asset.name.clone();
                        match doc.import_media(asset) {
                            Ok(_) => messages.push(format!("Added \"{name}\" to your media.")),
                            Err(e) => messages.push(format!("Could not add the sound: {e}")),
                        }
                    }
                    Err(e) => messages.push(format!("Could not read the new sound file: {e}")),
                },
                Err(e) => messages.push(format!("Could not extract the sound: {e}")),
            }
        }
        while let Ok(done) = self.receiver.try_recv() {
            self.busy.remove(&(done.clip, done.kind));
            let analysis = match done.result {
                Ok(a) => a,
                Err(e) => {
                    messages.push(format!("Could not measure the sound: {e}"));
                    continue;
                }
            };
            match done.kind {
                JobKind::Loudness => {
                    let lufs = analysis
                        .lufs
                        .unwrap_or(vz_audio::fx::loudness::SILENCE_LUFS);
                    let _ = doc.update_clip(done.clip, "Measure loudness", None, |c| {
                        let target = c
                            .audio
                            .normalize
                            .map_or(Normalize::SOCIAL, |n| n.target_lufs);
                        if c.audio.normalize.is_some() {
                            c.audio.normalize = Some(Normalize {
                                target_lufs: target,
                                measured_lufs: lufs,
                            });
                        }
                    });
                    if lufs <= vz_audio::fx::loudness::SILENCE_LUFS {
                        messages.push("This part is silent, so its loudness cannot be set.".into());
                    }
                }
                JobKind::Noise => match analysis.noise {
                    Some(profile) => {
                        let _ = doc.update_clip(done.clip, "Measure noise", None, |c| {
                            if let Some(d) = &mut c.audio.denoise {
                                d.profile = profile;
                            }
                        });
                    }
                    None => {
                        messages.push("Not enough sound to measure the background noise.".into());
                        let _ = doc.update_clip(done.clip, "Noise reduction", None, |c| {
                            c.audio.denoise = None;
                        });
                    }
                },
                JobKind::Beats => {
                    let beats = analysis.beats.unwrap_or_default();
                    if beats.times.is_empty() {
                        messages.push("No clear beat was found in this sound.".into());
                    } else {
                        messages.push(format!(
                            "Found {} beats at about {:.0} beats per minute.",
                            beats.times.len(),
                            beats.bpm
                        ));
                    }
                    let _ = doc.update_clip(done.clip, "Find beats", None, |c| {
                        c.audio.beats = beats.times;
                    });
                }
            }
        }
        messages
    }
}
