use super::*;
use vz_core::{BLACK, TextBackground, TextShadow, TextStroke, TextStyle, WHITE};

const SINHALA: [&str; 3] = ["ශ්‍රී ලංකාව", "ක්‍රීඩාව", "ඇස් සම්බන්ධ රෝග"];

fn text(s: &str) -> TextContent {
    TextContent::new(s)
}

fn styled(s: &str, f: impl FnOnce(&mut TextStyle)) -> TextContent {
    let mut t = TextContent::new(s);
    f(&mut t.style);
    t
}

fn alpha_sum(img: &TextImage) -> u64 {
    img.rgba.chunks(4).map(|p| u64::from(p[3])).sum()
}

/// (left, top, right, bottom) of the pixels that are not transparent.
fn bounds(img: &TextImage) -> (u32, u32, u32, u32) {
    let (mut l, mut t, mut r, mut b) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..img.height {
        for x in 0..img.width {
            if img.rgba[((y * img.width + x) * 4 + 3) as usize] > 20 {
                l = l.min(x);
                r = r.max(x);
                t = t.min(y);
                b = b.max(y);
            }
        }
    }
    (l, t, r, b)
}

fn dump(name: &str, img: &TextImage) {
    if let Ok(dir) = std::env::var("VZ_TEXT_DUMP") {
        let _ = std::fs::create_dir_all(&dir);
        // show on a dark grey background so white text is visible
        let mut flat = img.rgba.clone();
        for p in flat.chunks_mut(4) {
            let a = u32::from(p[3]);
            for c in &mut p[..3] {
                *c = ((u32::from(*c) * a + 40 * (255 - a)) / 255) as u8;
            }
            p[3] = 255;
        }
        image::save_buffer(
            format!("{dir}/{name}.png"),
            &flat,
            img.width,
            img.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

#[test]
fn latin_text_makes_a_picture_with_letters_in_it() {
    let mut e = TextEngine::new();
    let img = e.render(&text("Hello vidzcut"), 1.0);
    dump("latin", &img);
    assert!(alpha_sum(&img) > 5_000);
    let (l, t, r, b) = bounds(&img);
    assert!(r > l && b > t);
    // the letters sit inside the picture with room around them
    assert!(l >= 2 && t >= 2 && r < img.width - 2 && b < img.height - 2);
}

#[test]
fn sinhala_is_shaped_and_not_empty() {
    let mut e = TextEngine::new();
    for line in SINHALA {
        let img = e.render(&text(line), 1.0);
        dump(&format!("sinhala_{}", line.chars().count()), &img);
        assert!(alpha_sum(&img) > 5_000, "{line} draws something");
    }
}

#[test]
fn a_sinhala_letter_group_is_one_shape_not_many_letters() {
    // "ශ්‍රී" is 5 characters (ශ + ් + ZWJ + ර + ී); shaped, it becomes fewer pieces than characters
    let mut e = TextEngine::new();
    let laid = e.lay_out(&text("ශ්‍රී"), 1.0);
    let glyphs: usize = laid.buffer.layout_runs().map(|r| r.glyphs.len()).sum();
    assert!(glyphs < 5, "got {glyphs} glyphs");
    assert!(glyphs >= 1);
}

#[test]
fn tamil_works_too() {
    let mut e = TextEngine::new();
    let img = e.render(&text("தமிழ் எழுத்து"), 1.0);
    dump("tamil", &img);
    assert!(alpha_sum(&img) > 5_000);
}

#[test]
fn mixed_languages_in_one_line_work() {
    let mut e = TextEngine::new();
    let img = e.render(&text("Hello vidzcut — සිංහල + English"), 1.0);
    dump("mixed", &img);
    assert!(alpha_sum(&img) > 10_000);
}

#[test]
fn the_colour_of_the_text_is_used() {
    let mut e = TextEngine::new();
    let img = e.render(&styled("MMM", |s| s.color = [255, 0, 0, 255]), 1.0);
    let solid = img
        .rgba
        .chunks(4)
        .find(|p| p[3] == 255)
        .expect("a solid pixel");
    assert_eq!(&solid[..3], &[255, 0, 0]);
}

#[test]
fn bigger_text_makes_a_bigger_picture() {
    let mut e = TextEngine::new();
    let small = e.measure(&text("Hello"), 1.0);
    let big = e.measure(&styled("Hello", |s| s.size = 144.0), 1.0);
    assert!(big.0 > small.0 * 1.8 && big.1 > small.1 * 1.8);
    // the factor scales it the same way
    let scaled = e.measure(&text("Hello"), 2.0);
    assert!((scaled.0 / small.0 - 2.0).abs() < 0.1);
}

#[test]
fn bold_letters_are_wider() {
    let mut e = TextEngine::new();
    let normal = e.measure(&text("Hello world"), 1.0).0;
    let bold = e.measure(&styled("Hello world", |s| s.bold = true), 1.0).0;
    assert!(bold > normal);
}

#[test]
fn letter_spacing_widens_the_line() {
    let mut e = TextEngine::new();
    let normal = e.measure(&text("Spacing"), 1.0).0;
    let wide = e
        .measure(&styled("Spacing", |s| s.letter_spacing = 0.3), 1.0)
        .0;
    assert!(wide > normal * 1.3, "{wide} vs {normal}");
}

#[test]
fn more_lines_make_it_taller_and_line_spacing_adds_height() {
    let mut e = TextEngine::new();
    let one = e.measure(&text("One"), 1.0).1;
    let two = e.measure(&text("One\nTwo"), 1.0).1;
    assert!((two / one - 2.0).abs() < 0.05);
    let loose = e
        .measure(&styled("One\nTwo", |s| s.line_spacing = 1.5), 1.0)
        .1;
    assert!(loose > two * 1.4);
}

#[test]
fn a_text_box_wraps_long_text_at_its_width() {
    let mut e = TextEngine::new();
    let long = "This is a long headline that does not fit";
    let flat = e.measure(&text(long), 1.0);
    let wrapped = e.measure(&styled(long, |s| s.box_width = Some(300.0)), 1.0);
    assert!((wrapped.0 - 300.0).abs() < 1.0);
    assert!(wrapped.1 > flat.1 * 2.0);
}

#[test]
fn sinhala_wraps_between_words_without_cutting_a_letter_group() {
    let mut e = TextEngine::new();
    let t = styled("ඇස් සම්බන්ධ රෝග ශ්‍රී ලංකාව", |s| s.box_width = Some(260.0));
    let img = e.render(&t, 1.0);
    dump("sinhala_wrapped", &img);
    assert!(
        img.height as f32 > 2.0 * 72.0,
        "it needed more than one line"
    );
    let laid = e.lay_out(&t, 1.0);
    // every glyph's cluster starts and ends on a character boundary of the line
    for run in laid.buffer.layout_runs() {
        for g in run.glyphs {
            assert!(run.text.is_char_boundary(g.start) && run.text.is_char_boundary(g.end));
        }
    }
}

#[test]
fn alignment_moves_short_lines_inside_the_widest_one() {
    let mut e = TextEngine::new();
    let mk = |align| styled("A very long first line\nHi", move |s| s.align = align);
    let centre_of = |img: &TextImage| {
        // average x of the pixels in the bottom line (the short one)
        let (w, h) = (img.width, img.height);
        let (mut sum, mut n) = (0.0, 0.0);
        for y in (h * 2 / 3)..h {
            for x in 0..w {
                let a = f32::from(img.rgba[((y * w + x) * 4 + 3) as usize]);
                sum += x as f32 * a;
                n += a;
            }
        }
        sum / n.max(1.0)
    };
    let left = centre_of(&e.render(&mk(TextAlign::Left), 1.0));
    let mid = centre_of(&e.render(&mk(TextAlign::Center), 1.0));
    let right = centre_of(&e.render(&mk(TextAlign::Right), 1.0));
    assert!(
        left + 20.0 < mid && mid + 20.0 < right,
        "{left} {mid} {right}"
    );
}

#[test]
fn an_outline_makes_the_text_cover_more_and_keeps_the_fill_on_top() {
    let mut e = TextEngine::new();
    let plain = e.render(&text("Outline"), 1.0);
    let outlined = e.render(
        &styled("Outline", |s| {
            s.stroke = Some(TextStroke {
                color: BLACK,
                width: 6.0,
            })
        }),
        1.0,
    );
    dump("outline", &outlined);
    assert!(alpha_sum(&outlined) > alpha_sum(&plain) * 12 / 10);
    // black outline pixels exist, and white fill pixels still exist
    assert!(outlined.rgba.chunks(4).any(|p| p[3] > 250 && p[0] < 10));
    assert!(outlined.rgba.chunks(4).any(|p| p[3] > 250 && p[0] > 245));
}

#[test]
fn a_shadow_adds_soft_pixels_beside_the_text() {
    let mut e = TextEngine::new();
    let plain = e.render(&text("Shadow"), 1.0);
    let shadowed = e.render(
        &styled("Shadow", |s| {
            s.shadow = Some(TextShadow {
                color: [0, 0, 0, 200],
                blur: 8.0,
                dx: 6.0,
                dy: 6.0,
            })
        }),
        1.0,
    );
    dump("shadow", &shadowed);
    assert!(alpha_sum(&shadowed) > alpha_sum(&plain));
    assert!(shadowed.rgba.chunks(4).any(|p| p[3] > 20 && p[3] < 200));
}

#[test]
fn a_background_box_is_as_big_as_the_text_plus_padding() {
    let mut e = TextEngine::new();
    let plain = e.measure(&text("Box"), 1.0);
    let t = styled("Box", |s| {
        s.background = Some(TextBackground {
            color: [200, 30, 30, 255],
            padding: 20.0,
            radius: 12.0,
        })
    });
    let boxed = e.measure(&t, 1.0);
    assert!((boxed.0 - plain.0 - 40.0).abs() < 0.5);
    assert!((boxed.1 - plain.1 - 40.0).abs() < 0.5);
    let img = e.render(&t, 1.0);
    dump("background", &img);
    // the middle of the left padding is filled with the box colour, the corner is cut away by the radius
    let (mx, my) = (img.width / 2, img.height / 2);
    let mid = &img.rgba[((my * img.width + mx) * 4) as usize..][..4];
    assert!(mid[3] > 200);
    assert_eq!(
        img.rgba[3], 0,
        "the top left corner of the picture is empty"
    );
}

#[test]
fn underline_adds_a_line_under_the_letters() {
    let mut e = TextEngine::new();
    let plain = e.render(&text("Under"), 1.0);
    let underlined = e.render(&styled("Under", |s| s.underline = true), 1.0);
    dump("underline", &underlined);
    assert!(alpha_sum(&underlined) > alpha_sum(&plain));
}

#[test]
fn the_picture_is_centred_on_the_text_box() {
    let mut e = TextEngine::new();
    let img = e.render(
        &styled("Centre", |s| {
            s.stroke = Some(TextStroke {
                color: BLACK,
                width: 8.0,
            })
        }),
        1.0,
    );
    // the box inside the picture has the same room on every side
    let (bw, bh) = img.content;
    let room_x = (img.width as f32 - bw) / 2.0;
    let room_y = (img.height as f32 - bh) / 2.0;
    assert!(room_x >= 8.0 && room_y >= 8.0);
    assert!((room_x - room_y).abs() < 1.0);
}

#[test]
fn the_same_text_is_only_drawn_once() {
    let mut e = TextEngine::new();
    let t = text("Cached");
    let a = e.render(&t, 1.0);
    let b = e.render(&t, 1.0);
    assert!(Arc::ptr_eq(&a, &b));
    let c = e.render(&t, 2.0);
    assert!(!Arc::ptr_eq(&a, &c));
}

#[test]
fn empty_text_is_harmless() {
    let mut e = TextEngine::new();
    let img = e.render(&text(""), 1.0);
    assert!(img.width >= 1 && img.height >= 1);
    assert_eq!(alpha_sum(&img), 0);
}

#[test]
fn the_font_list_starts_with_ours() {
    let mut e = TextEngine::new();
    let families = e.families();
    assert_eq!(&families[..4], &BUNDLED_FAMILIES.map(String::from));
    let _ = WHITE;
}

#[test]
fn units_follow_the_short_side() {
    assert_eq!(unit(1920, 1080), 1.0);
    assert_eq!(unit(1080, 1920), 1.0);
    assert_eq!(unit(3840, 2160), 2.0);
}

/// Old-style Sinhala fonts (FM Abhaya, 4u-Nilakshi ...) put Sinhala letters on the Latin keys. Typing "dsdgrg@" in
/// such a font must draw Sinhala shapes, not the plain Latin letters. Only runs where such a font is installed.
#[test]
fn legacy_sinhala_fonts_draw_their_own_letters() {
    let mut e = TextEngine::new();
    let installed = e.families();
    let Some(legacy) = ["4u-Nilakshi", "FM Abhaya", "FM Arjunn", "FM Bindumathi"]
        .into_iter()
        .find(|f| installed.iter().any(|i| i == f))
    else {
        eprintln!("no legacy Sinhala font installed here, skipping");
        return;
    };
    let plain = e.render(&text("dsdgrg@"), 1.0);
    let in_legacy = e.render(&styled("dsdgrg@", |s| s.font = legacy.to_string()), 1.0);
    dump("legacy_plain", &plain);
    dump("legacy_font", &in_legacy);
    println!(
        "{legacy}: plain {}x{} alpha {}, legacy {}x{} alpha {}",
        plain.width,
        plain.height,
        alpha_sum(&plain),
        in_legacy.width,
        in_legacy.height,
        alpha_sum(&in_legacy)
    );
    assert_ne!(plain.rgba, in_legacy.rgba, "{legacy} was not used");
}

#[test]
#[ignore = "prints how an installed legacy font is built"]
fn show_how_a_legacy_font_is_built() {
    let name = std::env::var("VZ_FONT").unwrap_or_else(|_| "4u-Nilakshi".into());
    let mut e = TextEngine::new();
    let _ = e.families();
    let db = e.system().db();
    let faces: Vec<_> = db
        .faces()
        .filter(|f| {
            f.families
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case(&name))
        })
        .map(|f| {
            (
                f.id,
                format!(
                    "weight {} style {:?} stretch {:?}",
                    f.weight.0, f.style, f.stretch
                ),
                f.families.clone(),
            )
        })
        .collect();
    println!("{} face(s) named {name}: {faces:?}", faces.len());
    for (id, _, _) in faces {
        db.with_face_data(id, |data, index| {
            let face = ttf_parser::Face::parse(data, index).unwrap();
            println!("glyphs: {}", face.number_of_glyphs());
            for sub in face.tables().cmap.unwrap().subtables {
                println!(
                    "cmap platform {:?} encoding {} unicode {} | d={:?} F064={:?} 0D9A={:?}",
                    sub.platform_id,
                    sub.encoding_id,
                    sub.is_unicode(),
                    sub.glyph_index(u32::from('d')),
                    sub.glyph_index(0xF064),
                    sub.glyph_index(0x0D9A)
                );
            }
        });
    }
}

#[test]
#[ignore = "prints which font each letter really came from"]
fn show_which_font_draws_each_letter() {
    let name = std::env::var("VZ_FONT").unwrap_or_else(|_| "4u-Nilakshi".into());
    let mut e = TextEngine::new();
    let laid = e.lay_out(&styled("dsdgrg@", |s| s.font = name.clone()), 1.0);
    let db_names: Vec<(cosmic_text::fontdb::ID, String)> = e
        .system()
        .db()
        .faces()
        .map(|f| {
            (
                f.id,
                f.families
                    .first()
                    .map(|(n, _)| n.clone())
                    .unwrap_or_default(),
            )
        })
        .collect();
    for run in laid.buffer.layout_runs() {
        for g in run.glyphs {
            let who = db_names
                .iter()
                .find(|(id, _)| *id == g.font_id)
                .map(|(_, n)| n.as_str())
                .unwrap_or("?");
            println!("glyph id {} from font {who}", g.glyph_id);
        }
    }
}

fn typing(text_s: &str) -> TextContent {
    let mut t = styled(text_s, |s| s.size = 80.0);
    t.caret = true;
    t.animators = vec![vz_core::TextAnimator {
        dy: 0.0,
        delay: 0.2,
        duration: 0.01,
        easing: vz_anim::Easing::Linear,
        ..vz_core::TextAnimator::default()
    }];
    t
}

#[test]
fn the_typing_cursor_follows_the_letters_and_then_blinks() {
    let mut e = TextEngine::new();
    let t = typing("ABCDE");
    let secs = |s: f32| (s * 1_000_000.0) as i64;
    let total = secs(5.0);
    // nothing typed yet: only the cursor is there, on the left
    let start = e.render_at(&t, 1.0, 0, total);
    assert!(
        alpha_sum(&start) > 0,
        "the cursor shows before the first letter"
    );
    let two = e.render_at(&t, 1.0, secs(0.25), total);
    let four = e.render_at(&t, 1.0, secs(0.65), total);
    let (_, _, right_two, _) = bounds(&two);
    let (_, _, right_four, _) = bounds(&four);
    assert!(
        right_four > right_two,
        "more letters, the cursor is further right"
    );
    // all letters are there: the cursor blinks (on in the first half of every half... second, off in the next)
    let on = e.render_at(&t, 1.0, secs(2.1), total);
    let off = e.render_at(&t, 1.0, secs(2.6), total);
    assert!(alpha_sum(&on) > alpha_sum(&off), "cursor on, then off");
    // without the cursor switched on nothing changes
    let mut plain = t.clone();
    plain.caret = false;
    let no_caret = e.render_at(&plain, 1.0, secs(2.1), total);
    assert_eq!(alpha_sum(&no_caret), alpha_sum(&off));
}

#[test]
fn a_reflection_adds_a_fading_upside_down_copy_below() {
    let mut e = TextEngine::new();
    let plain = e.render(&styled("MIRROR", |s| s.size = 100.0), 1.0);
    let mut t = styled("MIRROR", |s| s.size = 100.0);
    t.reflection = Some(vz_core::Reflection::default());
    let with = e.render(&t, 1.0);
    assert!(
        with.height > plain.height + 40,
        "{} vs {}",
        with.height,
        plain.height
    );
    assert_eq!(with.width, plain.width);
    // the copy sits in the lower part and is dimmer than the letters
    let row_alpha = |img: &TextImage, y: u32| -> u32 {
        (0..img.width)
            .map(|x| u32::from(img.rgba[((y * img.width + x) * 4 + 3) as usize]))
            .max()
            .unwrap_or(0)
    };
    let h = with.height;
    let strongest_below = (h * 2 / 3..h).map(|y| row_alpha(&with, y)).max().unwrap();
    assert!(
        strongest_below > 30,
        "reflection is visible: {strongest_below}"
    );
    assert!(
        strongest_below < 200,
        "reflection is dimmer than the letters: {strongest_below}"
    );
    // the picture stays centred on the letters: the same room above and below
    let (_, top, _, bottom) = bounds(&with);
    assert!(top > 10 && h - bottom > 5);
}

#[test]
fn spread_moves_letters_outwards_and_flicker_is_the_same_every_time() {
    use vz_core::{TextAnimator, TextPhase};
    let an = TextAnimator {
        spread: 10.0,
        dy: 0.0,
        opacity: 1.0,
        delay: 0.0,
        duration: 1.0,
        ..TextAnimator::default()
    };
    let first = animate::state(&an, 0, 5, 0, 5_000_000);
    let last = animate::state(&an, 4, 5, 0, 5_000_000);
    let middle = animate::state(&an, 2, 5, 0, 5_000_000);
    assert_eq!((first.dx, last.dx, middle.dx), (-20.0, 20.0, 0.0));
    // the animation over: letters are home again
    assert_eq!(animate::state(&an, 0, 5, 2_000_000, 5_000_000).dx, 0.0);

    let flicker = TextAnimator {
        phase: TextPhase::Loop,
        flicker: true,
        dy: 0.0,
        opacity: 0.2,
        duration: 1.0,
        ..TextAnimator::default()
    };
    let samples: Vec<f32> = (0..120)
        .map(|i| animate::state(&flicker, 0, 1, i * 50_000, 6_000_000).opacity)
        .collect();
    let again: Vec<f32> = (0..120)
        .map(|i| animate::state(&flicker, 0, 1, i * 50_000, 6_000_000).opacity)
        .collect();
    assert_eq!(samples, again);
    let dips = samples.iter().filter(|o| **o < 0.99).count();
    assert!(
        dips > 5 && dips < 70,
        "it dips now and then, not always: {dips}/120"
    );
    assert!(samples.iter().all(|o| (0.2..=1.0).contains(o)));
}
