use super::*;
use serde_json::json;
use vz_core::TrackKind;

fn segment(material: &str, start: i64, length: i64) -> Value {
    json!({
        "material_id": material,
        "target_timerange": {"start": start, "duration": length},
        "source_timerange": {"start": 500_000, "duration": length},
        "speed": 1.0, "volume": 1.0, "visible": true,
        "clip": {
            "scale": {"x": 1.0, "y": 1.0}, "rotation": 0.0,
            "transform": {"x": 0.0, "y": 0.0},
            "flip": {"vertical": false, "horizontal": false}, "alpha": 1.0
        }
    })
}

fn draft() -> Value {
    let text_content = json!({
        "text": "ශ්‍රී ලංකාව 50%",
        "styles": [
            {"fill": {"content": {"solid": {"color": [1.0, 1.0, 1.0]}}}, "font": {"path": ""}, "size": 14, "range": [0, 9]},
            {"fill": {"content": {"solid": {"color": [1.0, 0.77, 0.0]}}}, "font": {"path": ""}, "size": 14, "range": [9, 12]}
        ]
    })
    .to_string();
    let mut intro = segment("v1", 0, 2_000_000);
    intro["clip"]["scale"] = json!({"x": 1.5, "y": 1.5});
    let mut slow = segment("v1", 2_000_000, 3_000_000);
    slow["speed"] = json!(1.5);
    slow["volume"] = json!(0.0);
    let mut bar = segment("s1", 0, 5_000_000);
    bar["clip"]["transform"] = json!({"x": 0.5, "y": 0.5});
    let mut title = segment("t1", 1_000_000, 3_000_000);
    title["clip"]["scale"] = json!({"x": 1.2, "y": 1.2});
    let mut group = segment("c1", 0, 5_000_000);
    group["extra_material_refs"] = json!(["d1"]);
    json!({
        "canvas_config": {"width": 1080, "height": 1920},
        "fps": 30.0,
        "tracks": [
            {"type": "video", "segments": [intro, slow]},
            {"type": "video", "segments": [bar]},
            {"type": "video", "segments": [group]},
            {"type": "text", "segments": [title]},
            {"type": "text", "segments": [segment("t2", 500_000, 1_500_000)]},
            {"type": "audio", "segments": [segment("a1", 0, 4_000_000)]},
            {"type": "effect", "segments": [segment("fx", 0, 1_000_000)]}
        ],
        "materials": {
            "videos": [
                {"id": "v1", "type": "video", "path": "Z:/definitely/not/here/clip.mp4", "duration": 20_000_000, "width": 1280, "height": 720, "has_audio": true},
                {"id": "c1", "type": "video", "path": "", "extra_type_option": 2, "material_name": "Compound clip1", "duration": 5_000_000, "width": 1080, "height": 1920}
            ],
            "audios": [{"id": "a1", "path": "Z:/definitely/not/here/voice.wav", "duration": 9_000_000}],
            "texts": [{
                "id": "t2", "type": "subtitle", "content": "{\"text\": \"caption words\", \"styles\": []}",
                "font_size": 8.0, "font_path": ""
            }, {
                "id": "t1", "content": text_content, "font_size": 14.0, "font_path": "",
                "alignment": 1, "line_spacing": 0.02, "line_max_width": 0.82, "text_color": "#ffffff",
                "has_shadow": true, "shadow_color": "#000000", "shadow_alpha": 0.5, "shadow_distance": 5.0,
                "shadow_point": {"x": 0.6, "y": -0.6}, "border_color": "#112233", "border_width": 0.08
            }],
            "shapes": [{
                "id": "s1", "shape_type": 4, "shape_size": [400.0, 100.0], "global_alpha": 1.0,
                "fill_render_style": {"color": {"solid": {"color": "#f1c600", "alpha": 1.0}}}
            }],
            "drafts": [{
                "id": "d1", "name": "",
                "draft": {
                    "canvas_config": {"width": 1080, "height": 1920}, "fps": 30.0,
                    "tracks": [{"type": "sticker", "segments": [segment("s2", 0, 5_000_000)]}],
                    "materials": {"shapes": [{
                        "id": "s2", "shape_type": 4, "shape_size": [200.0, 50.0],
                        "fill_render_style": {"color": {"solid": {"color": "#0900c3", "alpha": 1.0}}}
                    }]}
                }
            }]
        }
    })
}

fn find_clip(project: &Project, kind: TrackKind) -> &Clip {
    project
        .timeline()
        .tracks
        .iter()
        .filter(|t| t.kind == kind)
        .flat_map(|t| t.clips.iter())
        .next()
        .expect("a clip of that kind")
}

#[test]
fn a_draft_becomes_a_project_with_the_same_cuts() {
    let imported = convert(&draft(), "Test");
    let p = &imported.project;
    assert_eq!((p.canvas.width, p.canvas.height), (1080, 1920));
    assert_eq!(p.frame_rate, FrameRate::FPS_30);
    assert!(!p.auto_settings);

    let tl = p.timeline();
    let main = &tl.tracks[0];
    assert_eq!(main.kind, TrackKind::Main);
    assert_eq!(main.clips.len(), 2);
    assert_eq!(main.clips[0].start, 0);
    assert_eq!(
        main.clips[1].start,
        main.clips[0].end(),
        "no hole on the main track"
    );
    assert_eq!(main.clips[0].source_in, 500_000);
    // the file is used twice but listed once
    assert_eq!(
        p.media
            .iter()
            .filter(|m| m.kind == MediaKind::Video)
            .count(),
        1
    );
    assert_eq!(imported.missing, 2, "the video and the sound are not there");
}

#[test]
fn position_size_speed_and_sound_level_are_converted() {
    let p = convert(&draft(), "Test").project;
    let tl = p.timeline();
    let first = &tl.tracks[0].clips[0];
    assert_eq!(first.transform.scale_x, Animatable::Static(1.5));
    let slow = &tl.tracks[0].clips[1];
    assert_eq!(slow.speed, Animatable::Static(1.5));
    assert_eq!(slow.volume, Animatable::Static(0.0));

    // half a canvas up and right: +270 across, -480 (up is negative in vidzcut)
    let shape = find_clip(&p, TrackKind::Overlay);
    assert_eq!(shape.transform.x, Animatable::Static(270.0));
    assert_eq!(shape.transform.y, Animatable::Static(-480.0));
    let Content::Shape(s) = &shape.content else {
        panic!("a rectangle");
    };
    assert_eq!(s.kind, ShapeKind::Rectangle);
    assert_eq!(s.fill, Some([0xf1, 0xc6, 0x00, 255]));
}

#[test]
fn text_keeps_its_words_colours_and_look() {
    let p = convert(&draft(), "Test").project;
    let clip = find_clip(&p, TrackKind::Text);
    assert_eq!(clip.start, 1_000_000);
    let Content::Text(t) = &clip.content else {
        panic!("text");
    };
    assert_eq!(t.text, "ශ්‍රී ලංකාව 50%");
    assert_eq!(t.style.color, [255, 255, 255, 255]);
    // the yellow part is a span; the white part is the text's own colour
    assert_eq!(t.spans.len(), 1);
    assert_eq!(t.spans[0].color, Some([255, 196, 0, 255]));
    assert_eq!((t.spans[0].start, t.spans[0].end), (9, 12));
    assert!(t.style.shadow.is_some());
    assert!(t.style.stroke.is_some());
    assert!(t.style.box_width.is_some());
    assert!((t.style.size - 14.0 * TEXT_REF_PER_UNIT / DEFAULT_LINE_RATIO).abs() < 1e-3);
}

#[test]
fn a_compound_clip_becomes_a_nested_timeline() {
    let p = convert(&draft(), "Test").project;
    assert_eq!(p.timelines.len(), 2);
    let compound = p
        .timeline()
        .tracks
        .iter()
        .flat_map(|t| t.clips.iter())
        .find_map(|c| match c.content {
            Content::Compound(id) => Some(id),
            _ => None,
        })
        .expect("a compound clip");
    let inner = p
        .timeline_by_id(compound)
        .expect("its timeline is in the project");
    assert_eq!(inner.name, "Compound clip1");
    assert!(inner.tracks.iter().flat_map(|t| &t.clips).count() >= 1);
}

#[test]
fn what_cannot_come_along_is_reported_in_plain_words() {
    let imported = convert(&draft(), "Test");
    let all = imported.notes.join("\n");
    assert!(all.contains("effect items"), "{all}");
    assert!(all.contains("were not found"), "{all}");
}

#[test]
fn sound_tracks_come_last_and_the_main_track_first() {
    let p = convert(&draft(), "Test").project;
    let kinds: Vec<TrackKind> = p.timeline().tracks.iter().map(|t| t.kind).collect();
    assert_eq!(kinds[0], TrackKind::Main);
    assert_eq!(*kinds.last().unwrap(), TrackKind::Audio);
}

#[test]
fn a_locked_or_foreign_file_gives_a_plain_error() {
    let dir = std::env::temp_dir().join("vidzcut-capcut-test");
    std::fs::create_dir_all(&dir).unwrap();
    let locked = dir.join("locked.json");
    std::fs::write(&locked, [0x8f_u8, 0x12, 0x99, 0x03, 0x77]).unwrap();
    assert!(matches!(import_capcut(&locked), Err(ImportError::Locked)));
    let other = dir.join("other.json");
    std::fs::write(&other, r#"{"hello": 1}"#).unwrap();
    assert!(matches!(import_capcut(&other), Err(ImportError::Format(_))));
    assert!(matches!(
        import_capcut(&dir.join("missing.json")),
        Err(ImportError::Read(_))
    ));
}

#[test]
fn colours_and_rates_are_read_right() {
    assert_eq!(hex("#f1c600", 1.0), Some([0xf1, 0xc6, 0, 255]));
    assert_eq!(hex("#00000080", 1.0), Some([0, 0, 0, 128]));
    assert_eq!(hex("", 1.0), None);
    assert_eq!(hex("#ééé", 1.0), None);
    assert_eq!(nearest_rate(29.97), FrameRate::FPS_29_97);
    assert_eq!(nearest_rate(60.0), FrameRate::FPS_60);
    assert_eq!(nearest_rate(31.0), FrameRate::FPS_30);
}

/// Imports a real CapCut project (set `VZ_CAPCUT` to its folder) and prints what came out.
#[test]
#[ignore = "needs a real CapCut project"]
fn a_real_capcut_project() {
    let Ok(path) = std::env::var("VZ_CAPCUT") else {
        return;
    };
    let imported = import_capcut(Path::new(&path)).unwrap();
    let p = &imported.project;
    println!(
        "{}: {}x{} @ {:?}, {} media, {} timelines",
        p.name,
        p.canvas.width,
        p.canvas.height,
        p.frame_rate,
        p.media.len(),
        p.timelines.len()
    );
    for tl in &p.timelines {
        for t in &tl.tracks {
            println!("  {} {:?}: {} clips", tl.name, t.kind, t.clips.len());
        }
    }
    for n in &imported.notes {
        println!("note: {n}");
    }
}

/// Draws a picture of the real project (`VZ_CAPCUT` = its folder, `VZ_DUMP` = where to write) to compare with
/// CapCut's own cover picture.
#[test]
#[ignore = "needs a real CapCut project"]
fn a_picture_of_a_real_capcut_project() {
    let (Ok(path), Ok(dump)) = (std::env::var("VZ_CAPCUT"), std::env::var("VZ_DUMP")) else {
        return;
    };
    let project = import_capcut(Path::new(&path)).unwrap().project;
    let (w, h) = (720u32, 960u32);
    let mut renderer = vz_render::Renderer::new();
    std::fs::create_dir_all(&dump).unwrap();
    for (i, t) in [100_000_i64, 4_000_000].into_iter().enumerate() {
        let frame = renderer.render(&project, t, (w, h));
        let img = image::RgbaImage::from_raw(w, h, frame.rgba.clone()).unwrap();
        img.save(format!("{dump}/capcut_{i}.png")).unwrap();
    }
}

/// Shows how this computer's fonts are named (set `VZ_FONT` to a word of the name).
#[test]
#[ignore = "looks at this computer's fonts"]
fn show_font_names() {
    let word = std::env::var("VZ_FONT")
        .unwrap_or_else(|_| "Basuru".into())
        .to_lowercase();
    let mut engine = vz_text::TextEngine::new();
    for f in engine.families() {
        if f.to_lowercase().contains(&word) {
            println!("family: {f:?}");
        }
    }
    if let Ok(path) = std::env::var("VZ_FONT_FILE") {
        println!(
            "from the file: {:?}",
            vz_text::family_of_file(Path::new(&path))
        );
    }
}

/// Prints the font each text of a real project ended up with (`VZ_CAPCUT` = its folder).
#[test]
#[ignore = "needs a real CapCut project"]
fn the_fonts_of_a_real_capcut_project() {
    let Ok(path) = std::env::var("VZ_CAPCUT") else {
        return;
    };
    let project = import_capcut(Path::new(&path)).unwrap().project;
    for tl in &project.timelines {
        for clip in tl.tracks.iter().flat_map(|t| &t.clips) {
            if let Content::Text(t) = &clip.content {
                println!(
                    "{:?}: font {:?}, spans {:?}",
                    t.text,
                    t.style.font,
                    t.spans
                        .iter()
                        .map(|s| (&s.font, s.start, s.end))
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}

#[test]
fn capcut_captions_go_to_a_caption_track() {
    let p = convert(&draft(), "Test").project;
    let text_tracks: Vec<&Track> = p
        .timeline()
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Text)
        .collect();
    assert_eq!(text_tracks.len(), 2);
    assert_eq!(text_tracks.iter().filter(|t| t.captions).count(), 1);
    let captions = text_tracks.iter().find(|t| t.captions).unwrap();
    let Content::Text(t) = &captions.clips[0].content else {
        panic!("text");
    };
    assert_eq!(t.text, "caption words");
}

/// Tries every CapCut project on this computer and says which open and which do not.
#[test]
#[ignore = "needs CapCut projects on this computer"]
fn every_capcut_project_on_this_computer() {
    let mut worked = 0;
    for project in find_capcut_projects() {
        let started = std::time::Instant::now();
        match import_capcut(&project.folder) {
            Ok(imported) => {
                worked += 1;
                let clips: usize = imported
                    .project
                    .timelines
                    .iter()
                    .flat_map(|t| &t.tracks)
                    .map(|t| t.clips.len())
                    .sum();
                println!(
                    "ok   {:45} {:>4} clips, {} timelines, {} missing files, {:?}",
                    project.name,
                    clips,
                    imported.project.timelines.len(),
                    imported.missing,
                    started.elapsed()
                );
            }
            Err(e) => println!("FAIL {:45} {e}", project.name),
        }
    }
    println!("{worked} projects opened");
}
