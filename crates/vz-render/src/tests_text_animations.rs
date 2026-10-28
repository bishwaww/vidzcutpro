//! The animated text presets, drawn by the real renderer.

use super::*;
use vz_core::{Background, Document, Project, US_PER_SEC};
use vz_presets::text_animations::text_animations;

fn project_with(template: &vz_presets::templates::Template, w: u32, h: u32) -> Project {
    let mut project = Project::new("text animation");
    project.canvas = vz_core::Canvas {
        width: w,
        height: h,
    };
    project.background = Background::Color([230, 140, 60, 255]);
    let mut doc = Document::new(project);
    doc.add_group(template.build(w, h), 0, "add").unwrap();
    doc.project.clone()
}

fn bright(frame: &VideoFrame) -> usize {
    frame
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 200 && p[1] > 200 && p[2] > 200)
        .count()
}

#[test]
fn every_text_animation_draws_something_in_the_middle_and_stays_inside_its_length() {
    for template in text_animations() {
        let project = project_with(&template, 1280, 720);
        let mut r = Renderer::new();
        let mid = r.render(&project, template.duration * 6 / 10, (640, 360));
        let differs = mid
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| *p != [230, 140, 60, 255]);
        assert!(differs, "{} shows nothing at 60 %", template.name);
        // after its end only the backdrop is left
        let after = r.render(&project, template.duration + 2 * US_PER_SEC, (640, 360));
        assert!(
            after
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [230, 140, 60, 255]),
            "{} is still on screen after its end",
            template.name
        );
    }
}

#[test]
fn the_typewriter_types_and_the_video_in_text_lets_the_picture_through() {
    let all = text_animations();
    let typewriter = all.iter().find(|t| t.id == "typewriter").unwrap();
    let project = project_with(typewriter, 1280, 720);
    let mut r = Renderer::new();
    let early = r.render(&project, 600_000, (640, 360));
    let late = r.render(&project, 4_000_000, (640, 360));
    let ink = |f: &VideoFrame| {
        f.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] > 240 && p[1] > 240 && p[2] > 240)
            .count()
    };
    assert!(
        ink(&late) > ink(&early) * 3,
        "{} vs {}",
        ink(&early),
        ink(&late)
    );

    // video in text: black everywhere except inside the letters, where the backdrop shows
    let all2 = text_animations();
    let vit = all2.iter().find(|t| t.id == "video_in_text").unwrap();
    let project = project_with(vit, 1280, 720);
    let frame = r.render(&project, 2_000_000, (640, 360));
    let px = frame.rgba.as_chunks::<4>().0;
    let black = px
        .iter()
        .filter(|p| p[0] < 8 && p[1] < 8 && p[2] < 8)
        .count();
    let backdrop = px.iter().filter(|p| p[0] > 200 && p[1] > 100).count();
    assert!(
        black > px.len() / 3,
        "the box is black: {black}/{}",
        px.len()
    );
    assert!(backdrop > 2000, "the letters show the picture: {backdrop}");
}

/// Writes contact sheets of every preset to `VZ_DUMP` (run with --ignored).
#[test]
#[ignore = "writes pictures for a person to look at"]
fn dump_the_text_animations() {
    let Ok(dir) = std::env::var("VZ_DUMP") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    for template in text_animations() {
        let project = project_with(&template, 1280, 720);
        let mut r = Renderer::new();
        let times: Vec<i64> = (0..8)
            .map(|i| template.duration * i / 7)
            .map(|t| t.min(template.duration - 100_000))
            .collect();
        let tw = std::env::var("VZ_TILE")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(400);
        let th = tw * 9 / 16;
        let mut sheet = image::RgbaImage::new(tw * 4, th * 2);
        for (i, t) in times.iter().enumerate() {
            let f = r.render(&project, *t, (tw, th));
            let img = image::RgbaImage::from_raw(tw, th, f.rgba.clone()).unwrap();
            image::imageops::overlay(
                &mut sheet,
                &img,
                i64::from(tw) * (i as i64 % 4),
                i64::from(th) * (i as i64 / 4),
            );
        }
        sheet.save(format!("{dir}/{}.png", template.id)).unwrap();
    }
    let _ = bright;
}
