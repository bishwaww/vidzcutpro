//! A background thread that imports files and makes thumbnails, so the window never waits for them.

use crate::decoder::{FrameSource, MaxSize, VideoFrame};
use crate::probe::probe;
use crossbeam_channel::{Receiver, Sender, unbounded};
use std::path::PathBuf;
use std::sync::Arc;
use vz_core::{MediaAsset, MediaId, MediaKind, Micros, US_PER_SEC};

/// The biggest thumbnail we make.
const THUMB_BOX: MaxSize = MaxSize {
    width: 320,
    height: 320,
    enlarge: false,
};

pub enum Job {
    /// Look inside a file and make its first thumbnail.
    Import(PathBuf),
    /// Make a thumbnail for media that is already in a project (thumbnails are not saved in the project).
    Poster {
        id: MediaId,
        path: PathBuf,
        kind: MediaKind,
        duration: Micros,
    },
}

pub enum Event {
    Imported {
        asset: MediaAsset,
        poster: Option<Arc<VideoFrame>>,
    },
    ImportFailed {
        path: PathBuf,
        error: String,
    },
    Poster {
        id: MediaId,
        poster: Arc<VideoFrame>,
    },
}

pub struct MediaWorker {
    jobs: Sender<Job>,
    events: Receiver<Event>,
}

impl MediaWorker {
    /// Start the worker. `wake` is called whenever a result is ready (the window uses it to redraw).
    pub fn start(wake: impl Fn() + Send + 'static) -> Self {
        let (jobs, job_rx) = unbounded::<Job>();
        let (event_tx, events) = unbounded::<Event>();
        std::thread::Builder::new()
            .name("vidzcut-media".into())
            .spawn(move || {
                for job in job_rx {
                    let event = match job {
                        Job::Import(path) => match probe(&path) {
                            Ok(asset) => {
                                let poster = make_poster(&asset.path, asset.kind, asset.duration);
                                Event::Imported { asset, poster }
                            }
                            Err(e) => Event::ImportFailed {
                                path,
                                error: e.to_string(),
                            },
                        },
                        Job::Poster {
                            id,
                            path,
                            kind,
                            duration,
                        } => match make_poster(&path, kind, duration) {
                            Some(poster) => Event::Poster { id, poster },
                            None => continue,
                        },
                    };
                    if event_tx.send(event).is_err() {
                        break; // the window is gone
                    }
                    wake();
                }
            })
            .expect("could not start the media thread");
        Self { jobs, events }
    }

    pub fn import(&self, path: PathBuf) {
        let _ = self.jobs.send(Job::Import(path));
    }

    pub fn poster(&self, id: MediaId, path: PathBuf, kind: MediaKind, duration: Micros) {
        let _ = self.jobs.send(Job::Poster {
            id,
            path,
            kind,
            duration,
        });
    }

    /// Everything finished since the last call.
    pub fn poll(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }
}

/// A representative picture for a media file (a frame a little way in, so it is not the black first frame).
pub fn make_poster(
    path: &std::path::Path,
    kind: MediaKind,
    duration: Micros,
) -> Option<Arc<VideoFrame>> {
    if kind == MediaKind::Audio {
        return None;
    }
    let mut source = FrameSource::open(path, THUMB_BOX)
        .inspect_err(|e| tracing::warn!("no thumbnail for {}: {e}", path.display()))
        .ok()?;
    let at = (duration / 10).min(3 * US_PER_SEC);
    source.frame_at(at).ok().flatten()
}
