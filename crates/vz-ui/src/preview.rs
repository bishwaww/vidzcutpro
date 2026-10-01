//! Draws preview pictures on a background thread, so scrubbing and playing never freeze the window.
//!
//! The window says "I want the picture at time T"; it may say that many times a second. The thread only
//! ever draws the newest request (older ones are skipped), which is what keeps playback smooth.

use crossbeam_channel::{Receiver, Sender, unbounded};
use std::sync::{Arc, Mutex};
use vz_core::{Micros, Project};
use vz_media::VideoFrame;
use vz_render::Renderer;

enum Msg {
    Project(Arc<Project>),
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
    while let Ok(first) = rx.recv() {
        // take everything that is waiting; only the newest of each kind matters
        let mut newest_render = None;
        for msg in std::iter::once(first).chain(rx.try_iter()) {
            match msg {
                Msg::Project(p) => project = Some(p),
                Msg::Render { t, size, serial } => newest_render = Some((t, size, serial)),
            }
        }
        let (Some(project), Some((t, size, serial))) = (&project, newest_render) else {
            continue;
        };
        let frame = renderer.render(project, t, size);
        if let Ok(mut shared) = out.lock() {
            shared.frame = Some((serial, frame));
        }
        wake();
    }
}
