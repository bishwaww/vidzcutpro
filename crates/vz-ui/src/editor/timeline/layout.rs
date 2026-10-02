//! Which track is drawn where. Tracks stack top to bottom like in other editors: overlay and text tracks
//! on top (the newest highest), then the main track, then the audio tracks.

use eframe::egui::{Pos2, Rect};
use vz_core::{Micros, Timeline, TrackFlags, TrackId, TrackKind, US_PER_SEC};

pub const HEADER_W: f32 = 132.0;
pub const RULER_H: f32 = 26.0;
/// Space above the first and below the last track where a dropped clip creates a new track.
pub const ZONE_H: f32 = 18.0;
pub const ROW_GAP: f32 = 4.0;
/// How close to a clip's edge (in points) a click counts as grabbing the edge.
pub const EDGE_GRAB: f32 = 7.0;

pub fn row_height(kind: TrackKind) -> f32 {
    match kind {
        TrackKind::Main => 74.0,
        TrackKind::Overlay => 54.0,
        TrackKind::Text => 36.0,
        TrackKind::Audio => 50.0,
    }
}

/// One track as it is laid out. `top` counts from the top of all the tracks (before scrolling).
#[derive(Clone, Debug)]
pub struct Row {
    pub track: TrackId,
    pub kind: TrackKind,
    pub name: String,
    pub flags: TrackFlags,
    pub top: f32,
    pub height: f32,
}

/// The rows in drawing order, and the height of everything.
pub fn build_rows(timeline: &Timeline) -> (Vec<Row>, f32) {
    let visual_above = timeline
        .tracks
        .iter()
        .rev()
        .filter(|t| t.kind.is_visual() && t.kind != TrackKind::Main);
    let main = timeline.tracks.iter().filter(|t| t.kind == TrackKind::Main);
    let audio = timeline
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Audio);
    let mut top = ZONE_H;
    let mut rows = Vec::new();
    for t in visual_above.chain(main).chain(audio) {
        let height = row_height(t.kind);
        rows.push(Row {
            track: t.id,
            kind: t.kind,
            name: t.name.clone(),
            flags: TrackFlags::of(t),
            top,
            height,
        });
        top += height + ROW_GAP;
    }
    (rows, top + ZONE_H)
}

/// A place a dropped clip can create a new track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    /// Above every track: a new overlay track.
    Top,
    /// Below every track: a new audio track.
    Bottom,
}

/// Converts between time and screen position for the current zoom and scroll.
#[derive(Clone, Copy, Debug)]
pub struct Geo {
    pub lane_x0: f32,
    pub pps: f32,
    pub scroll_x: f32,
}

impl Geo {
    pub fn x(self, t: Micros) -> f32 {
        self.lane_x0 + (t as f64 / US_PER_SEC as f64) as f32 * self.pps - self.scroll_x
    }

    pub fn time(self, x: f32) -> Micros {
        (f64::from(x - self.lane_x0 + self.scroll_x) / f64::from(self.pps) * US_PER_SEC as f64)
            as Micros
    }

    /// A length in points as a length of time.
    pub fn span(self, dx: f32) -> Micros {
        (f64::from(dx) / f64::from(self.pps) * US_PER_SEC as f64) as Micros
    }
}

/// Everything about where things are on screen for one frame.
pub struct Frame {
    pub area: Rect,
    pub ruler: Rect,
    pub geo: Geo,
    /// Screen y of the top of the first track row (already includes scrolling).
    pub origin_y: f32,
    pub rows: Vec<Row>,
}

impl Frame {
    pub fn row_rect(&self, row: &Row) -> Rect {
        let y = self.origin_y + row.top;
        Rect::from_min_max(
            Pos2::new(self.geo.lane_x0, y),
            Pos2::new(self.area.max.x, y + row.height),
        )
    }

    pub fn header_rect(&self, row: &Row) -> Rect {
        let y = self.origin_y + row.top;
        Rect::from_min_max(
            Pos2::new(self.area.min.x, y),
            Pos2::new(self.geo.lane_x0, y + row.height),
        )
    }

    /// The track row under a screen y (lanes and headers alike).
    pub fn row_at(&self, y: f32) -> Option<&Row> {
        self.rows.iter().find(|r| {
            let top = self.origin_y + r.top;
            y >= top - ROW_GAP / 2.0 && y < top + r.height + ROW_GAP / 2.0
        })
    }

    /// Is this y in the strip above all tracks or below all of them?
    pub fn zone_at(&self, y: f32) -> Option<Zone> {
        let first = self.rows.first()?;
        let last = self.rows.last()?;
        if y < self.origin_y + first.top - ROW_GAP / 2.0 {
            Some(Zone::Top)
        } else if y >= self.origin_y + last.top + last.height + ROW_GAP / 2.0 {
            Some(Zone::Bottom)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vz_core::{Track, TrackKind};

    #[test]
    fn tracks_stack_overlays_on_top_then_main_then_audio() {
        let mut tl = Timeline::new("t");
        tl.tracks.push(Track::new(TrackKind::Overlay, "Overlay 1"));
        tl.tracks.push(Track::new(TrackKind::Audio, "Audio 1"));
        tl.tracks.push(Track::new(TrackKind::Text, "Text 1"));
        tl.tracks.push(Track::new(TrackKind::Overlay, "Overlay 2"));
        let (rows, height) = build_rows(&tl);
        let names: Vec<_> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Overlay 2", "Text 1", "Overlay 1", "Main", "Audio 1"]
        );
        assert!(
            rows.windows(2).all(|w| w[0].top + w[0].height < w[1].top),
            "rows do not overlap"
        );
        let last = rows.last().unwrap();
        assert!(height > last.top + last.height);
        assert_eq!(rows[0].top, ZONE_H);
    }

    #[test]
    fn a_new_timeline_has_just_the_main_row() {
        let (rows, _) = build_rows(&Timeline::new("t"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, TrackKind::Main);
    }
}
