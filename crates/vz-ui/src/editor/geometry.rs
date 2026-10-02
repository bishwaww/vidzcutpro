//! Where a clip is on the canvas, worked out the same way the renderer does it. The player's handles,
//! clicking and the inspector's alignment buttons all use this.

use eframe::egui::{Pos2, Vec2, vec2};
use vz_core::{Clip, Content, Micros, Project};

/// A layer's placement in canvas pixels (the canvas runs from (0, 0) at the top left to its width and height).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub center: Pos2,
    /// The picture's size before scaling (a picture fitted into the canvas; text at its own size).
    pub size: Vec2,
    pub scale: Vec2,
    /// Degrees, clockwise.
    pub rotation: f32,
}

impl Placed {
    /// The four corners (top left, top right, bottom right, bottom left) after scaling and turning.
    pub fn corners(&self) -> [Pos2; 4] {
        let (hw, hh) = (
            self.size.x * self.scale.x.abs() / 2.0,
            self.size.y * self.scale.y.abs() / 2.0,
        );
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)].map(|(x, y)| {
            Pos2::new(
                self.center.x + x * cos - y * sin,
                self.center.y + x * sin + y * cos,
            )
        })
    }

    /// Is a canvas point on the layer?
    pub fn contains(&self, p: Pos2) -> bool {
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        let (dx, dy) = (p.x - self.center.x, p.y - self.center.y);
        let (lx, ly) = (dx * cos + dy * sin, -dx * sin + dy * cos);
        lx.abs() <= self.size.x * self.scale.x.abs() / 2.0
            && ly.abs() <= self.size.y * self.scale.y.abs() / 2.0
    }

    /// The size on the canvas after scaling.
    pub fn scaled_size(&self) -> Vec2 {
        vec2(
            self.size.x * self.scale.x.abs(),
            self.size.y * self.scale.y.abs(),
        )
    }
}

/// The size a picture clip has at scale 1: its media fitted into the canvas.
pub fn base_size(project: &Project, clip: &Clip) -> Option<Vec2> {
    let Content::Media(media) = &clip.content else {
        return None;
    };
    let asset = project.asset(*media)?;
    if !asset.has_picture() || asset.width == 0 || asset.height == 0 {
        return None;
    }
    let (cw, ch) = (project.canvas.width as f32, project.canvas.height as f32);
    let k = (cw / asset.width as f32).min(ch / asset.height as f32);
    Some(vec2(asset.width as f32 * k, asset.height as f32 * k))
}

/// Where `clip` is on the canvas at timeline time `t`. `None` for clips with no picture (sound; text until its size is known).
pub fn placed(project: &Project, clip: &Clip, t: Micros) -> Option<Placed> {
    let size = base_size(project, clip)?;
    let tr = clip.transform.at(clip.local_time(t));
    Some(Placed {
        center: Pos2::new(
            project.canvas.width as f32 / 2.0 + tr.x,
            project.canvas.height as f32 / 2.0 + tr.y,
        ),
        size,
        scale: vec2(tr.scale_x, tr.scale_y),
        rotation: tr.rotation,
    })
}

/// The top-most picture clip at canvas point `p` at time `t`.
pub fn pick(project: &Project, t: Micros, p: Pos2) -> Option<vz_core::ClipId> {
    project
        .timeline()
        .layers_at(t)
        .iter()
        .rev()
        .find(|layer| placed(project, layer.clip, t).is_some_and(|pl| pl.contains(p)))
        .map(|layer| layer.clip.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vz_core::{Document, FrameRate, MediaAsset, MediaId, MediaKind, TrackKind};

    fn doc_with_clip() -> (Document, vz_core::ClipId) {
        let mut doc = Document::new(Project::new("t"));
        let asset = MediaAsset {
            id: MediaId::new(),
            name: "a".into(),
            path: "/a.mp4".into(),
            relative_path: None,
            kind: MediaKind::Video,
            duration: 10_000_000,
            width: 1080,
            height: 1080,
            frame_rate: Some(FrameRate::FPS_30),
            has_audio: false,
            file_size: 1,
        };
        let m = doc.import_media(asset).unwrap();
        // a square video on a 1920x1080 canvas: set the canvas by hand, the clip is on an overlay
        doc.set_aspect(vz_core::AspectPreset::Wide16x9).unwrap();
        let overlay = doc.add_track(TrackKind::Overlay).unwrap();
        let id = doc.place_media(overlay, m, 0).unwrap();
        (doc, id)
    }

    #[test]
    fn a_square_picture_is_fitted_into_a_wide_canvas() {
        let (doc, id) = doc_with_clip();
        let clip = doc.project.timeline().clip(id).unwrap();
        let p = placed(&doc.project, clip, 0).unwrap();
        assert_eq!((p.size.x, p.size.y), (1080.0, 1080.0));
        assert_eq!((p.center.x, p.center.y), (960.0, 540.0));
    }

    #[test]
    fn points_inside_and_outside_are_told_apart_even_when_turned() {
        let (mut doc, id) = doc_with_clip();
        doc.update_clip(id, "x", None, |c| {
            c.transform.scale_x = 0.5.into();
            c.transform.rotation = 45.0.into();
        })
        .unwrap();
        let p = placed(&doc.project, doc.project.timeline().clip(id).unwrap(), 0).unwrap();
        assert!(p.contains(Pos2::new(960.0, 540.0)));
        // 540 wide (half of 1080) and 1080 tall, turned 45 degrees: a point straight up the turned axis is inside
        assert!(p.contains(Pos2::new(
            960.0 + 300.0 * std::f32::consts::FRAC_1_SQRT_2,
            540.0 - 300.0 * std::f32::consts::FRAC_1_SQRT_2
        )));
        assert!(!p.contains(Pos2::new(960.0 + 400.0, 540.0)));
        let c = p.corners();
        assert!(c.iter().all(|k| (k.x - 960.0).hypot(k.y - 540.0) > 200.0));
    }

    #[test]
    fn the_top_most_clip_under_a_point_is_picked() {
        let (mut doc, under) = doc_with_clip();
        let overlay2 = doc.add_track(TrackKind::Overlay).unwrap();
        let media = doc.project.media[0].id;
        let over = doc.place_media(overlay2, media, 0).unwrap();
        assert_eq!(pick(&doc.project, 0, Pos2::new(960.0, 540.0)), Some(over));
        assert_eq!(
            pick(&doc.project, 0, Pos2::new(5.0, 5.0)),
            None,
            "the corner is outside the square"
        );
        doc.set_track_flags(overlay2, |f| f.hidden = true).unwrap();
        assert_eq!(pick(&doc.project, 0, Pos2::new(960.0, 540.0)), Some(under));
    }
}
