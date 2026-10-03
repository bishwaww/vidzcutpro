//! Measures the loudness of audio files in the background, so audio clips can show their waveform.

use crossbeam_channel::{Receiver, Sender};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use vz_audio::Waveform;
use vz_core::{MediaId, MediaKind, Project};

pub struct Waveforms {
    done: HashMap<MediaId, Arc<Waveform>>,
    pending: HashSet<MediaId>,
    sender: Sender<(MediaId, Option<Waveform>)>,
    receiver: Receiver<(MediaId, Option<Waveform>)>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Waveforms {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded();
        Self {
            done: HashMap::new(),
            pending: HashSet::new(),
            sender,
            receiver,
            wake: Arc::new(wake),
        }
    }

    /// Start measuring any audio file of the project that has not been measured yet, and collect finished ones.
    pub fn update(&mut self, project: &Project) {
        while let Ok((id, wave)) = self.receiver.try_recv() {
            self.pending.remove(&id);
            if let Some(w) = wave {
                self.done.insert(id, Arc::new(w));
            }
        }
        for asset in &project.media {
            if asset.kind != MediaKind::Audio
                || !asset.has_audio
                || self.done.contains_key(&asset.id)
                || self.pending.contains(&asset.id)
            {
                continue;
            }
            self.pending.insert(asset.id);
            let (id, path, duration) = (asset.id, asset.path.clone(), asset.duration);
            let (sender, wake) = (self.sender.clone(), self.wake.clone());
            std::thread::spawn(move || {
                // a failed file gets an empty waveform, so it is not tried again and again
                let wave = match vz_audio::waveform::compute(&path, duration, None) {
                    Ok(w) => w,
                    Err(e) => {
                        tracing::warn!("no waveform for {}: {e}", path.display());
                        Some(Waveform::new(Vec::new()))
                    }
                };
                let _ = sender.send((id, wave));
                wake();
            });
        }
    }

    pub fn clear(&mut self) {
        self.done.clear();
    }

    pub fn map(&self) -> &HashMap<MediaId, Arc<Waveform>> {
        &self.done
    }
}
