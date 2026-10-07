//! Draws preview pictures on a background thread, so scrubbing and playing never freeze the window.
//!
//! The window says "I want the picture at time T"; it may say that many times a second. The thread only
//! ever draws the newest request (older ones are skipped), which is what keeps playback smooth.

use crossbeam_channel::{Receiver, Sender, unbounded};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use vz_core::{Micros, Project};
use vz_media::VideoFrame;
use vz_render::Renderer;

enum Msg {
    Project(Arc<Project>),
    /// How much memory the picture cache may use, in bytes.
    CacheLimit(usize),
    /// Original file → smaller stand-in copy to use while previewing.
    Proxies(HashMap<std::path::PathBuf, std::path::PathBuf>),
    Render {
        t: Micros,
        size: (u32, u32),
        serial: u64,
    },
}

#[derive(Default)]
struct Shared {
    frame: Option<(u64, Arc<VideoFrame>)>,
}

pub struct Preview {
    tx: Sender<Msg>,
    shared: Arc<Mutex<Shared>>,
    serial: u64,
    seen: u64,
}

impl Preview {
    pub fn start(wake: impl Fn() + Send + 'static) -> Self {
        let (tx, rx) = unbounded::<Msg>();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let out = shared.clone();
        std::thread::Builder::new()
            .name("vidzcut-preview".into())
            .spawn(move || worker(rx, out, wake))
            .expect("could not start the preview thread");
        Self {
            tx,
            shared,
            serial: 0,
            seen: 0,
        }
    }

    /// Tell the preview about the current project (call after every change).
    pub fn set_project(&self, project: Arc<Project>) {
        let _ = self.tx.send(Msg::Project(project));
    }

    /// Tell the preview how much memory its picture cache may use.
    pub fn set_cache_limit(&self, bytes: usize) {
        let _ = self.tx.send(Msg::CacheLimit(bytes));
    }

    /// Tell the preview which small stand-in copies to use instead of big original videos.
    pub fn set_proxies(&self, map: HashMap<std::path::PathBuf, std::path::PathBuf>) {
        let _ = self.tx.send(Msg::Proxies(map));
    }

    /// Ask for the picture at time `t`, `size` pixels.
    pub fn request(&mut self, t: Micros, size: (u32, u32)) {
        self.serial += 1;
        let _ = self.tx.send(Msg::Render {
            t,
            size,
            serial: self.serial,
        });
    }

    /// A new picture, if one finished since the last call.
    pub fn take_new_frame(&mut self) -> Option<Arc<VideoFrame>> {
        let guard = self.shared.lock().ok()?;
        match &guard.frame {
            Some((serial, frame)) if *serial != self.seen => {
                self.seen = *serial;
                Some(frame.clone())
            }
            _ => None,
        }
    }
}

fn worker(rx: Receiver<Msg>, out: Arc<Mutex<Shared>>, wake: impl Fn()) {
    let mut renderer = Renderer::new();
    let mut project: Option<Arc<Project>> = None;
    let mut cache = FrameCache::new(DEFAULT_CACHE_BYTES);
    while let Ok(first) = rx.recv() {
        // take everything that is waiting; only the newest of each kind matters
        let mut newest_render = None;
        for msg in std::iter::once(first).chain(rx.try_iter()) {
            match msg {
                Msg::Project(p) => {
                    project = Some(p);
                    cache.clear();
                }
                Msg::CacheLimit(bytes) => cache.set_limit(bytes),
                Msg::Proxies(map) => {
                    renderer.set_proxies(map);
                    cache.clear();
                }
                Msg::Render { t, size, serial } => newest_render = Some((t, size, serial)),
            }
        }
        let (Some(project), Some((t, size, serial))) = (&project, newest_render) else {
            continue;
        };
        let frame = match cache.get(t, size) {
            Some(f) => f,
            None => {
                let f = renderer.render(project, t, size);
                cache.insert(t, size, f.clone());
                f
            }
        };
        if let Ok(mut shared) = out.lock() {
            shared.frame = Some((serial, frame));
        }
        wake();
    }
}

/// Until Settings says otherwise: 400 MB.
const DEFAULT_CACHE_BYTES: usize = 400 * 1024 * 1024;

/// Pictures already drawn for the project as it is now. Going back over the same part (scrubbing, looping, pausing
/// and playing again) then costs nothing. Thrown away whenever the project changes.
pub struct FrameCache {
    pictures: HashMap<(Micros, (u32, u32)), Arc<VideoFrame>>,
    /// Oldest first, to know what to drop when the cache is full.
    order: VecDeque<(Micros, (u32, u32))>,
    bytes: usize,
    limit: usize,
}

impl FrameCache {
    pub fn new(limit_bytes: usize) -> Self {
        Self {
            pictures: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            limit: limit_bytes,
        }
    }

    pub fn set_limit(&mut self, limit_bytes: usize) {
        self.limit = limit_bytes;
        self.trim();
    }

    pub fn clear(&mut self) {
        self.pictures.clear();
        self.order.clear();
        self.bytes = 0;
    }

    pub fn get(&self, t: Micros, size: (u32, u32)) -> Option<Arc<VideoFrame>> {
        self.pictures.get(&(t, size)).cloned()
    }

    pub fn insert(&mut self, t: Micros, size: (u32, u32), frame: Arc<VideoFrame>) {
        let key = (t, size);
        if self.pictures.contains_key(&key) || frame.rgba.len() > self.limit {
            return;
        }
        self.bytes += frame.rgba.len();
        self.pictures.insert(key, frame);
        self.order.push_back(key);
        self.trim();
    }

    fn trim(&mut self) {
        while self.bytes > self.limit {
            let Some(old) = self.order.pop_front() else {
                break;
            };
            if let Some(f) = self.pictures.remove(&old) {
                self.bytes -= f.rgba.len();
            }
        }
    }
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.pictures.len()
    }
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.pictures.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(bytes: usize) -> Arc<VideoFrame> {
        Arc::new(VideoFrame {
            width: 1,
            height: 1,
            rgba: vec![0; bytes],
            pts: 0,
        })
    }

    #[test]
    fn the_cache_remembers_pictures_by_time_and_size() {
        let mut c = FrameCache::new(1000);
        c.insert(10, (4, 4), picture(100));
        assert!(c.get(10, (4, 4)).is_some());
        assert!(c.get(10, (8, 8)).is_none());
        assert!(c.get(11, (4, 4)).is_none());
        c.clear();
        assert!(c.is_empty());
    }

    #[test]
    fn the_oldest_pictures_go_first_when_it_is_full() {
        let mut c = FrameCache::new(250);
        for t in 0..5 {
            c.insert(t, (1, 1), picture(100));
        }
        assert_eq!(c.len(), 2);
        assert!(c.get(0, (1, 1)).is_none());
        assert!(c.get(4, (1, 1)).is_some());
        c.set_limit(100);
        assert_eq!(c.len(), 1);
        // a picture bigger than the whole cache is not kept
        c.insert(9, (1, 1), picture(500));
        assert!(c.get(9, (1, 1)).is_none());
    }
}
