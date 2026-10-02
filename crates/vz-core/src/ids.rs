//! Stable ids. Every media file, track and clip has one, so undo, copy/paste and saving can point at things safely.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

id_type!(
    /// A media file that was imported into the project.
    MediaId
);
id_type!(
    /// One timeline (a project can have several).
    TimelineId
);
id_type!(
    /// One track on a timeline.
    TrackId
);
id_type!(
    /// One clip on a track.
    ClipId
);
id_type!(
    /// Clips that were grouped so they select and move together.
    GroupId
);
id_type!(
    /// Clips that are linked (for example a video and its separated sound) so they move together.
    LinkId
);
id_type!(
    /// A marker on the timeline.
    MarkerId
);
