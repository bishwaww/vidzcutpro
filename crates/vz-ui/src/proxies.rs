//! Makes proxies (small, quick copies of big videos) in the background, one at a time, and says which exist.

use crossbeam_channel::{Receiver, Sender};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use vz_core::{FrameRate, MediaAsset, MediaId, MediaKind, Project};
use vz_export::{AUTO_PROXY_ABOVE, make_proxy, proxy_path};

/// Where proxies are kept.
pub fn proxy_dir() -> PathBuf {
    crate::editor::audio_dir()
        .parent()
        .map_or_else(|| PathBuf::from("proxies"), |p| p.join("proxies"))
}

/// Does this video deserve a proxy without being asked? (Bigger than full HD.)
pub fn wants_proxy(asset: &MediaAsset) -> bool {
    asset.kind == MediaKind::Video
        && asset.width.max(asset.height) > AUTO_PROXY_ABOVE
        && asset.duration > 0
}

struct Done {
    id: MediaId,
    path: PathBuf,
    ok: bool,
}

pub struct Proxies {
    folder: PathBuf,
    /// The proxy file of every video that has one.
    ready: HashMap<MediaId, PathBuf>,
    /// Which file each media id was last looked at with (so a relink is noticed).
    looked_at: HashMap<MediaId, PathBuf>,
    queue: VecDeque<(MediaId, MediaAsset)>,
    queued: HashSet<MediaId>,
    failed: HashSet<MediaId>,
    /// The one being made now: its name and how far it is (0 to 1000).
    busy: Option<(MediaId, String, Arc<AtomicU32>, Arc<AtomicBool>)>,
    sender: Sender<Done>,
    receiver: Receiver<Done>,
    wake: Arc<dyn Fn() + Send + Sync>,
    /// Goes up whenever the set of ready proxies changes.
    pub generation: u64,
}

impl Proxies {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded();
        Self {
            folder: proxy_dir(),
            ready: HashMap::new(),
            looked_at: HashMap::new(),
            queue: VecDeque::new(),
            queued: HashSet::new(),
            failed: HashSet::new(),
            busy: None,
            sender,
            receiver,
            wake: Arc::new(wake),
            generation: 0,
        }
    }

    fn path_for(&self, asset: &MediaAsset) -> PathBuf {
        proxy_path(&self.folder, &asset.path, asset.file_size, asset.duration)
    }

    /// Look at the project's videos: note existing proxies, queue the big ones if `auto`, and start the next job.
    pub fn update(&mut self, project: &Project, auto: bool) {
        while let Ok(done) = self.receiver.try_recv() {
            self.busy = None;
            self.queued.remove(&done.id);
            if done.ok {
                self.ready.insert(done.id, done.path);
                self.generation += 1;
            } else {
                self.failed.insert(done.id);
            }
        }
        for asset in &project.media {
            if asset.kind != MediaKind::Video {
                continue;
            }
            if self.looked_at.get(&asset.id) != Some(&asset.path) {
                self.looked_at.insert(asset.id, asset.path.clone());
                self.failed.remove(&asset.id);
                if self.ready.remove(&asset.id).is_some() {
                    self.generation += 1;
                }
                let path = self.path_for(asset);
                if path.exists() {
                    self.ready.insert(asset.id, path);
                    self.generation += 1;
                }
            }
            if auto
                && wants_proxy(asset)
                && !self.ready.contains_key(&asset.id)
                && !self.failed.contains(&asset.id)
            {
                self.enqueue(asset);
            }
        }
        // forget videos that left the project
        let alive: HashSet<MediaId> = project.media.iter().map(|m| m.id).collect();
        self.queue.retain(|(id, _)| alive.contains(id));
        self.queued
            .retain(|id| alive.contains(id) || self.busy.as_ref().is_some_and(|b| b.0 == *id));
        self.start_next();
    }

    fn enqueue(&mut self, asset: &MediaAsset) {
        if self.queued.insert(asset.id) {
            self.queue.push_back((asset.id, asset.clone()));
        }
    }

    /// Make a proxy of this video now (also when it is not big, or an earlier try failed).
    pub fn request(&mut self, project: &Project, id: MediaId) {
        if let Some(asset) = project.asset(id)
            && asset.kind == MediaKind::Video
            && !self.ready.contains_key(&id)
        {
            self.failed.remove(&id);
            self.enqueue(asset);
        }
    }

    fn start_next(&mut self) {
        if self.busy.is_some() {
            return;
        }
        let Some((id, asset)) = self.queue.pop_front() else {
            return;
        };
        let dest = self.path_for(&asset);
        let progress = Arc::new(AtomicU32::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        self.busy = Some((id, asset.name.clone(), progress.clone(), cancel.clone()));
        let (sender, wake) = (self.sender.clone(), self.wake.clone());
        let rate = asset
            .frame_rate
            .filter(|r| r.as_f64() <= 60.5)
            .unwrap_or(FrameRate::FPS_30);
        std::thread::Builder::new()
            .name("vidzcut-proxy".into())
            .spawn(move || {
                let result = make_proxy(
                    &asset.path,
                    &dest,
                    asset.duration,
                    rate,
                    &mut |p| {
                        let share = p.frames_done * 1000 / p.frames_total.max(1);
                        progress.store(share as u32, Ordering::Relaxed);
                        wake();
                    },
                    &cancel,
                );
                if let Err(e) = &result {
                    tracing::warn!("proxy of {} failed: {e}", asset.path.display());
                }
                let _ = sender.send(Done {
                    id,
                    path: dest,
                    ok: result.is_ok(),
                });
                wake();
            })
            .expect("could not start the proxy thread");
    }

    /// Stop making proxies (the one running and those waiting).
    pub fn cancel_all(&mut self) {
        self.queue.clear();
        if let Some((_, _, _, cancel)) = &self.busy {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Delete every proxy file.
    pub fn delete_all(&mut self) {
        self.cancel_all();
        let _ = std::fs::remove_dir_all(&self.folder);
        self.ready.clear();
        self.looked_at.clear();
        self.queued.clear();
        self.generation += 1;
    }

    /// Original → proxy for every video that has one.
    pub fn map(&self, project: &Project) -> HashMap<PathBuf, PathBuf> {
        project
            .media
            .iter()
            .filter_map(|a| Some((a.path.clone(), self.ready.get(&a.id)?.clone())))
            .collect()
    }

    /// One line for the Details panel, or `None` when there is nothing to say.
    pub fn status(&self) -> Option<String> {
        if let Some((_, name, progress, _)) = &self.busy {
            let percent = progress.load(Ordering::Relaxed) / 10;
            let waiting = self.queue.len();
            return Some(if waiting > 0 {
                format!("Making a proxy of {name}: {percent} %  ({waiting} more waiting)")
            } else {
                format!("Making a proxy of {name}: {percent} %")
            });
        }
        (!self.ready.is_empty()).then(|| format!("{} proxy file(s) ready", self.ready.len()))
    }
}
