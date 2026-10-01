# VIDZCUT — Master Specification (Rust rebuild)

> This file is the single source of truth for building **vidzcut**, a desktop video editor for **Windows and macOS**, written in **Rust**, whose interface and workflow are modeled on **CapCut PC**, with extra keyframe / motion-graphics depth inspired by **Premiere Pro**.
> Read this whole file before planning or writing any code. When this file and your own idea disagree, follow this file and tell the owner.
>
> (Saved verbatim from the owner's message of 2026-10-01. Do not edit except when the owner changes the spec; record changes in the "Spec changes" list at the bottom.)

---

## 0. Who you are working for and how to work

The owner is **not a programmer**. You are the senior developer. That means:

1. **Explain in plain language.** No jargon without a one-line explanation. When you need a decision, give 2–3 options, say which you recommend and why, and wait.
2. **Never ask the owner to edit code.** If something must be run, give the exact command to copy-paste and say what they should see.
3. **Work in phases** (Section 12). Finish one phase, make sure it builds and runs, then **stop** and give the owner a short, numbered "How to test this" checklist written for a non-programmer. Do not start the next phase until the owner says OK.
4. **Keep the project healthy every time you stop:**
   - `cargo fmt`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, `cargo build --release` all pass.
   - Update `ROADMAP.md` (tick finished items) and `CLAUDE.md` (what exists, where it lives, known issues, next step) so a fresh session can continue.
   - Commit to git with a clear message. Small, frequent commits.
5. **Never leave the app in a broken state.** A missing feature is fine; a crash is not. If something can't be done well yet, hide it behind a disabled menu item labelled "Coming soon".
6. **Ask before** adding any paid service, any network/cloud feature, any GPL-licensed dependency, or anything that changes the project file format in a breaking way.

### Scope rules
- **No AI / machine-learning / cloud features in this version.** Excluded: auto captions (speech-to-text), AI background removal, AI cutout, retouch/beauty, AI avatars, text-to-speech, AI color match, smart suggestions, AI clipper, auto reframe, AI effects, copyright check. Design an extension point (Section 11.4) so they can be added later as plugins.
- Everything else in CapCut PC that is **non-AI** is in scope, plus the motion-graphics extras in Section 9.

### Legal / originality rules (important)
- CapCut is a **reference for layout and behaviour only**. Do **not** copy, extract, decompile, or reuse anything from the CapCut application: no code, binaries, icons, UI images, fonts, effects, filters/LUTs, stickers, templates, music, sound effects, or the CapCut name.
- If the owner places a CapCut install folder in the project, **ignore it** and tell the owner it isn't needed.
- All built-in assets (icons, effects, transitions, LUTs, stickers, text styles, sounds) must be **made by us** (procedural / shader-based / hand-written data files) or come from sources with a license that allows redistribution in a commercial product (MIT, Apache-2.0, BSD, OFL for fonts, CC0). Record every third-party asset and crate license in `THIRD_PARTY_LICENSES.md`.
- Keep the codebase **release-safe**: prefer MIT/Apache crates; use an **LGPL build of FFmpeg, dynamically linked**; do **not** pull in GPL components (x264, x265, Rubber Band, GPL FFmpeg builds) without asking.

### Branding
- App name: **vidzcut**. Use the **existing vidzcut logo/icon** from the old prototype (Section 1).
- Visual identity: dark UI with vidzcut's **yellow/amber accent** (as in the prototype), not CapCut's teal. Use an open icon set (e.g. Lucide, ISC license) or draw our own icons.

---

## 1. The old prototype (read first, then build from scratch)

The owner's earlier prototype lives at **`D:\Projects\vidzcut`** (the owner may give a different path — use what they give).

Do this before planning:
1. Read the prototype's code, `CLAUDE.md` / `AGENTS.md` and any docs.
2. Write `docs/PROTOTYPE_NOTES.md` covering: features it already has, how its UI is laid out, how it renders text (especially **Sinhala**), how it handles captions, compound clips, speed and animations, what looked fragile, and which ideas are worth keeping.
3. Copy the **logo / app icon / brand assets** into `assets/brand/` of the new project.
4. **Do not copy its architecture.** The new project is a clean rebuild in Rust following this spec. You may port small algorithms if they are good and you rewrite them to fit the new design.

The prototype already showed (from the owner's screen recording): an asset panel with caption style presets (Karaoke Yellow, Green Pop, Reels 3 Words, One Word Pop, Word Reveal, Red Keyword), Sinhala captions on video, a Speed panel (Normal/Curve, 0.5×–3× buttons, keep pitch, reverse, freeze frame), an Animation panel (In/Out/Combo with Fade, Zoom, Slide, Spin, Swing, Pop, Blur), multiple timeline tabs, and compound clips. The new build must reach at least that level and then go far beyond it.

---

## 2. Target platforms and quality bar

- **Windows 10/11 x64** and **macOS 12+ (Apple Silicon and Intel)**. One codebase.
- Smooth 1080p30 preview of a 3–4 track project on a mid-range laptop with integrated graphics; 4K editable via proxies.
- UI stays responsive at all times (never freezes while decoding, exporting, generating thumbnails, etc.).
- What you see in the player is **exactly** what is exported (same render code path).
- Crash-safe: autosave + recovery on next launch.
- Complex-script text must shape correctly — **Sinhala** is a first-class requirement (the owner's audience is Sri Lankan). Also Tamil and English. Test with real Sinhala sentences including conjuncts and vowel signs (e.g. "ශ්‍රී ලංකාව", "ක්‍රීඩාව", "ඇස් සම්බන්ධ රෝග").

---

## 3. Technology stack (recommended — justify in ARCHITECTURE.md if you change anything)

| Area | Choice | Notes |
|---|---|---|
| Language | Rust stable, Cargo workspace | edition 2021+ |
| Window + UI | `eframe` / `egui` with the **wgpu** backend | Pure Rust, cross-platform, good for custom-drawn timelines. `egui_dock` or own splitter code for resizable panels. |
| GPU | `wgpu` + WGSL shaders | Compositing, effects, transitions, color, masks. Preview texture drawn directly inside egui (no copy back to CPU). |
| Decode / encode | FFmpeg via `ffmpeg-next` (or `ffmpeg-sys-next`) | LGPL shared build, bundled DLLs/dylibs. Hardware decode: D3D11VA (Win), VideoToolbox (mac). Hardware encode: NVENC / QSV / AMF (Win), VideoToolbox (mac). |
| Audio output | `cpal` | Audio clock is the master playback clock. |
| Resampling | `rubato` | |
| Loudness | `ebur128` | Loudness normalization (EBU R128). |
| Time-stretch / pitch | Own WSOLA/phase-vocoder implementation or an LGPL/MIT library | For speed changes with "keep pitch" and voice effects. No GPL. |
| Text shaping | `cosmic-text` (+ `fontdb`, `swash`) | Must shape Sinhala correctly. Bundle Noto Sans/Serif Sinhala, Noto Sans Tamil, Inter (OFL). Load system fonts too. |
| 2D vector | `lyon` (tessellation → wgpu) or `vello` / `tiny-skia` | Shapes, pen paths, strokes, gradients, text outlines. |
| Images | `image`, `resvg` (SVG stickers), GIF/WebP/APNG decode | |
| Serialization | `serde`, `serde_json` | Project files, presets. |
| File dialogs | `rfd` | Native open/save dialogs. |
| Concurrency | `crossbeam-channel`, `rayon`, `parking_lot` | |
| Logging / errors | `tracing` (+ file log), `anyhow`, `thiserror` | |
| Paths | `directories` | Cache, config, autosave locations. |
| Packaging | `cargo-packager` (or equivalent) | Windows installer (NSIS/MSI) and macOS `.app`/`.dmg` (universal). |

Verify current crate versions before adding them. Prefer few, well-maintained dependencies.

---

## 4. Architecture

### 4.1 Workspace layout
```
vidzcut/
  Cargo.toml                (workspace)
  crates/
    vz-core/      project model, timeline logic, time math, commands + undo/redo, serialization, migrations (NO UI, NO GPU — heavily unit-tested)
    vz-media/     probing, decoding, frame cache, thumbnails, waveforms, proxies, hardware accel
    vz-render/    wgpu compositor, render graph, WGSL shaders (effects, transitions, color, masks, blend modes)
    vz-text/      fonts, shaping, text layout, text styles, caption layout
    vz-vector/    shapes, pen paths, gradients, stroke/fill rasterization
    vz-audio/     playback engine, mixer, DSP (gain, fades, EQ, noise reduction, pitch, stretch, loudness)
    vz-anim/      keyframes, easing, bezier curves, animation presets (in/out/combo/loop), motion paths
    vz-export/    offline render loop → encoder, export presets, GIF/image/audio/subtitle export
    vz-presets/   built-in preset data: transitions, effects, filters/LUTs, animations, text styles, caption styles, stickers (as data + shaders)
    vz-ui/        all egui panels and widgets
    vidzcut/      the binary: app startup, window, menus, wiring
  assets/         brand, icons, fonts, built-in LUTs, stickers, sample media for tests
  docs/           ARCHITECTURE.md, PROTOTYPE_NOTES.md, BUILDING.md, TESTING.md
  ROADMAP.md  CLAUDE.md  THIRD_PARTY_LICENSES.md
```

### 4.2 Core rules
- **Time is integers.** Use `i64` ticks (microseconds) in the model. Never `f32`/`f64` seconds for positions/durations. Snap to the project frame grid when the user edits. Support 23.976/29.97 correctly via rational frame rates.
- **Data model** (all items have stable UUIDs):
  - `Project` → settings, `MediaPool` (assets by id, with relative + absolute paths for relinking), `Vec<Timeline>`, presets used, cover.
  - `Timeline` → `Vec<Track>`; one **main track** (magnetic) + any number of overlay video/image/text/sticker/shape/effect/filter/adjustment tracks + audio tracks.
  - `Clip` kinds: `Video`, `Image`, `Audio`, `Text`, `Caption`, `Sticker`, `Shape`, `Effect`, `Filter`, `Adjustment`, `Compound(timeline_id)`, `SolidColor`.
  - `Transition` sits between two adjacent clips on the same track.
  - Every animatable property is `Animatable<T> = Static(T) | Keyframed(Vec<Keyframe<T>>)`, where a keyframe has time, value, easing in/out (preset or custom bezier).
- **Commands + undo/redo:** every edit is a `Command` with `apply`/`revert`. Unlimited history. Continuous drags merge into one undo step. The UI never mutates the model directly.
- **One render path:** `evaluate(timeline, t) → layer stack → GPU composite`. Preview and export both call this. Effects/transitions/color are WGSL passes chained in a render graph with texture pooling.
- **Threads:** UI thread never blocks. Decoder workers per active clip with a look-ahead GPU frame cache (LRU). Real-time-safe audio thread (no allocation, no locks that can block). Background job system for thumbnails, waveforms, proxies, loudness analysis, export — each with progress and cancel.
- **Playback sync:** audio clock is master; video drops frames if late; preview resolution can be Full / 1/2 / 1/4 automatically or by user.
- **Project file:** `.vzp` (JSON) with `format_version` and migration code. Autosave every 30 s (configurable) to a recovery folder + "Auto saved: HH:MM:SS" indicator. Missing media → "Relink" dialog, never a crash.
- **Plugins/extension point:** a `trait FeatureProvider` (e.g. caption generator, background remover) registered at startup, so AI tools can be added later without touching the core.

---

## 5. Application screens

### 5.1 Home screen (project manager)
- Big **"Create project"** button.
- Grid/list of recent projects: thumbnail (cover), name, duration, last modified, size. Search, sort by date/name.
- Right-click: Open, Rename, Duplicate, Delete (to recycle bin), Show in folder, Export project folder (with media, "Collect files").
- Import project file. Open from recovery after a crash.
- Settings button, app version, logo.

### 5.2 Editor layout (match CapCut PC's structure)
```
┌──────────────────────────────────────────────────────────────────────────────┐
│ [logo] Menu ▾   • Auto saved: 12:34:11      Project name     [layout][⌨][Export]│
├───────────────────────────────┬───────────────────────┬──────────────────────┤
│ Media Audio Text Stickers      │      Player           │   Inspector          │
│ Effects Transitions Captions   │  (canvas + controls)  │ (tabs change with    │
│ Filters Adjustment Templates   │                       │  the selection)      │
│ ┌────────┬──────────────────┐ │                       │                      │
│ │category│  asset grid      │ │                       │                      │
│ │sidebar │                  │ │                       │                      │
│ └────────┴──────────────────┘ │                       │                      │
├───────────────────────────────┴───────────────────────┴──────────────────────┤
│ [Timeline 01] [Timeline 02] [Timeline 03] [+]                                 │
│ toolbar: select/blade · undo redo · split · split L/R · delete · freeze ·     │
│          reverse · mirror · rotate · crop · marker   |  magnet · snap ·       │
│          linkage · preview-axis · zoom slider · fit                           │
│ ┌─ track headers ─┬──────────── ruler / tracks / clips / playhead ──────────┐ │
│ │ lock hide mute  │ text / sticker / effect / overlay tracks                │ │
│ │ [Cover]         │ main track (thumbnails filmstrip)                       │ │
│ │                 │ audio tracks (waveforms)                                │ │
│ └─────────────────┴──────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────┘
```
- All panel borders are draggable; sizes saved per user.
- **Layout presets** button (as in CapCut): default (player centre, inspector right), player on the right with inspector in the middle, larger timeline, and "reset layout".
- When nothing is selected the inspector shows **Project / Details**: project name, resolution & aspect, frame rate, color space (Rec.709 SDR), proxy on/off, where the project and media live, and non-AI **Global edits** (e.g. "Normalize loudness of all clips", "Apply a filter to all clips", "Apply transition to all cuts").
- Top-bar **Menu**: File (New, Open, Save, Save as, Import media, Export, Collect files, Recent, Close project), Edit (Undo, Redo, Cut, Copy, Paste, Paste attributes, Delete, Select all, Group/Ungroup, Create compound clip), View (layouts, zoom, show waveforms/thumbnails, safe zones), Settings, Help (shortcuts, about, open log folder).

---

## 6. Asset panel (top-left) — every tab

Each tab = left category sidebar + searchable grid. Every item has: hover preview (animated where relevant), a "+" button to add at playhead, drag-and-drop onto timeline/player, right-click "Add to favourites". A **Favourites** category exists in every tab.

### 6.1 Media
- **Import** button (files and folders) and drag-and-drop from Explorer/Finder. Formats: MP4, MOV, MKV, WebM, AVI, MTS/M2TS, HEVC, ProRes (decode), MP3, WAV, AAC, M4A, FLAC, OGG, JPG, PNG, WebP, GIF, BMP, TIFF, HEIC (if FFmpeg build supports), SVG.
- Sub-sections: **Local** (project media), **Library** (our own built-in clips: solid colors, gradients, countdowns, transparent placeholder, simple procedural backgrounds), **Subprojects** (other timelines / compound clips in this project).
- Grid or list view; sort (name, date added, duration, type); filter by type; thumbnail size slider.
- Hover-scrub preview on video thumbnails; duration badge; **"Added"** badge when already used on the timeline.
- Right-click: Show in folder, Relink, Replace media, Generate proxy, Delete from project, Properties (codec, resolution, fps, bitrate, audio channels).

### 6.2 Audio
- **Music** and **Sound effects** from user-chosen local folders (we ship a small set of our own/CC0 sound effects: whoosh, pop, click, swoosh, ding, camera shutter, typing, riser, impact).
- **Extract audio** from a video file into the project.
- **Record voiceover** (microphone select, input level meter, countdown, records onto a new audio track at the playhead).
- Preview play on hover/click, waveform thumbnail, duration.

### 6.3 Text
- **Add text** (default text box).
- **Text styles / effects** (the "ART" grid): preset combinations of fill, gradient, stroke(s), shadow, glow, background — all built-in, ours.
- **Text templates**: multi-layer animated title designs (lower thirds, subscribe banners, headline boxes, "breaking news" bars, quote cards, countdown titles), each a saved group of text + shapes + animations that stays editable.
- Categories: Trending, Basic, Bold, Neon, Outline, Gradient, Retro, Lower thirds, Titles, Social, Sinhala-friendly (styles tested with Sinhala fonts).
- **Fonts**: list system fonts + bundled fonts with preview; import .ttf/.otf; favourites.

### 6.4 Stickers
- Our own sticker set: emoji-style, arrows, circles/highlights, speech bubbles, badges ("NEW", "SALE", "SUBSCRIBE"), social icons drawn by us, sparkles, hearts.
- Animated stickers (our own, as GIF/APNG/WebP or procedural).
- Import PNG/GIF/WebP/APNG/SVG as stickers. Lottie (.json) import is a later nice-to-have.
- **Shapes** category (vector): rectangle, rounded rectangle, circle/ellipse, triangle, parallelogram, trapezoid, star, polygon, line, arrow, and a **pen tool** for custom shapes ("double-click the preview to finish drawing", as CapCut does).

### 6.5 Effects (video effects, all non-AI, all shader-based)
Each effect is a clip on an effect track (applies to everything below it, or to a chosen clip), with adjustable parameters and keyframes.
Categories and examples:
- **Basic:** blur, zoom blur, radial blur, directional/motion blur, sharpen, vignette, film grain, letterbox bars, mirror, flip, split-screen 2/3/4/9.
- **Light & lens:** light leaks (procedural), lens flare, glow/bloom, flash, strobe, sparkle, bokeh, chromatic aberration, prism.
- **Retro:** VHS, old film (scratches, dust, flicker), black & white, sepia, CRT scanlines, 8-bit pixelate, halftone, film burn.
- **Distortion:** shake (camera shake with intensity/frequency), wave, swirl, fisheye, bulge/pinch, ripple, glitch, RGB split, digital noise, kaleidoscope, tile.
- **Party/energy:** pulse zoom, beat bounce, flicker, neon edge, rainbow, color cycle.
- **Frames/overlays:** borders, rounded corners, drop shadow, picture-frame, phone/browser mock frame (our design).
- Each: intensity, speed, plus effect-specific parameters; "Apply to: main track / all / selected clip".

### 6.6 Transitions
Applied between two adjacent clips (drag onto the cut). Duration adjustable (handles on timeline + inspector), "Apply to all cuts".
- **Basic:** mix/dissolve, fade through black, fade through white, blur dissolve.
- **Movement:** slide (L/R/U/D), push, whip pan (with motion blur), zoom in, zoom out, spin, pull in/out, shake.
- **Wipes:** linear wipe (any angle), clock wipe, circle/iris, diamond, barn door, blinds, split.
- **Effects:** glitch, flash, light leak, RGB split, pixelate, burn, ink/smoke-style (procedural noise), ripple.
- **3D:** cube, flip, page turn, door.
- Each with easing and direction options.

### 6.7 Captions (non-AI)
- **Add caption manually** (adds a caption at playhead on a caption track) and **caption editor list**: every caption line in a list with start/end times; edit text inline; merge/split lines; find & replace; shift all timings.
- **Import captions**: SRT, VTT, LRC, ASS (basic styling), TXT (one line per caption with even spacing).
- **Export captions**: SRT, TXT, VTT.
- **Caption styles** (from the prototype): Karaoke Yellow, Green Pop, Reels 3 Words, One Word Pop, Word Reveal, Red Keyword, plus more. Word-level effects work when word timings exist (from imported files or the user marking words), or are estimated by splitting the line duration across words.
- **Apply style to all captions** toggle (as in CapCut).
- Keyword highlighting: user marks words → highlight color/scale/animation.
- Leave the **plugin hook** for future auto-captions (Section 11.4), but no speech recognition now.

### 6.8 Filters
- Built-in **LUT-based color looks** we create ourselves (warm, cool, cinematic teal-orange, film, vintage, moody, bright, pastel, B&W variants, food, landscape, portrait, night). Categories like CapCut: Featured, Life, Landscape, Portrait, Movies, Retro, Food, B&W, Night.
- **Import .cube LUTs** (1D/3D).
- Filter strength 0–100. Apply as a clip on a filter track (affects all below) or directly to a selected clip.

### 6.9 Adjustment
- **Adjustment layer** clip: holds color adjustments/effects and affects all tracks below for its duration.
- Saved **adjustment presets** ("My presets").

### 6.10 Templates
- **Save as template**: save the current timeline (or a selected group) as a reusable template with replaceable media slots and editable texts.
- **Use template**: fill slots with the user's own media.
- Ship a few of our own templates (intro, outro, subscribe end-screen, lower third pack, 9:16 short format).

---

## 7. Player (canvas)

- Play/pause, current time / total duration (HH:MM:SS:FF), frame step ←/→, jump to start/end, loop playback, mark in/out.
- **Aspect ratio / canvas**: Original, 16:9, 9:16, 1:1, 4:3, 3:4, 4:5, 21:9, 2.35:1, custom. Changing it keeps clips sensibly placed.
- **Background** for empty canvas areas: solid color, blurred copy of the clip (blur amount), image, pattern.
- Zoom (fit, 25–400 %), pan, full-screen preview, preview quality (Auto / Full / 1/2 / 1/4), toggle proxies.
- **Direct manipulation**: click a layer to select it; bounding box with handles to move, scale (Shift = keep ratio), rotate (snap 15° with Shift); crop mode on the canvas; edit mask shapes on the canvas; double-click text to edit in place.
- **Smart guides**: snap to canvas centre/edges/other layers, with pink guide lines; optional safe zones (title/action safe, TikTok/Reels/Shorts UI overlays drawn by us as outlines); rule-of-thirds grid.
- Show **motion path** for keyframed position (Section 9).
- Player title shows which timeline is playing (e.g. "Player – Timeline 03").

---

## 8. Timeline

### 8.1 Structure
- Multiple **timelines per project** shown as tabs ("Timeline 01", "+" to add; rename, duplicate, delete; drag a timeline into another as a compound clip).
- Tracks: main video track (magnetic), overlay tracks (video/image/text/sticker/shape), effect tracks, filter tracks, adjustment tracks, audio tracks. Unlimited tracks; reorder by drag; track height adjustable; collapse.
- Track header: **lock**, **hide** (eye), **mute**, track name/color; main track has **Cover** button (choose a frame or an image as the project cover).
- Clips show: name, duration, filmstrip thumbnails (video), waveform (audio, also on video clips with audio), icon + text for text/sticker/effects, color per type, badges (speed changed, reversed, keyframes, effects applied), keyframe diamonds when selected.

### 8.2 Toolbar & tools (CapCut + Premiere)
- **Selection tool (A)** and **Blade/Split tool (B)**.
- Undo / Redo.
- **Split at playhead** (Ctrl/Cmd+B), **Split – delete left (Q)**, **Split – delete right (W)**.
- **Delete** and **Ripple delete** (Shift+Delete), close gap.
- **Freeze frame** (inserts still of the current frame, default 3 s).
- **Reverse**, **Mirror**, **Rotate 90°**, **Crop** (opens crop dialog with ratio presets + free crop).
- **Markers** on timeline and on clips (with color and note); jump between markers.
- **Main track magnet** on/off, **auto snapping** on/off (to playhead, clip edges, markers, beats), **linkage** on/off (linked audio/video move together), **preview axis** (hover-scrub the timeline).
- Zoom slider, zoom to fit (Shift+Z), +/- keys.

### 8.3 Editing behaviour
- Drag clips between tracks; overwrite vs. insert behaviour; magnetic main track closes gaps automatically.
- Trim edges by dragging; **ripple trim**, **roll edit**, **slip** (move content inside the clip), **slide** (move clip between neighbours) — Premiere-style tools available via shortcut/modifier.
- Multi-select (box select, Ctrl/Cmd+click, select all on track after playhead); **group/ungroup**; **create compound clip** (Alt+G) / open compound clip in its own tab / **uncompound**.
- **Detach / extract audio** from video (J/L cuts by unlinking).
- Copy / paste / duplicate; **copy & paste attributes** (effects, color, animations, keyframes, transform) to other clips.
- Replace clip (keeps duration and effects).
- Nudge selected clips by one frame (, / . keys) or 10 frames with Shift.
- Beat markers: **auto beat detection** on music clips (signal-processing onset detection, not AI) and manual beat tapping; clips snap to beats.
- Playhead follows playback; auto-scroll; ruler shows timecode with frame ticks.
- Right-click menus with all relevant actions per clip type.

---

## 9. Inspector (right panel) — every clip type

Every numeric control: slider + number field, scrub-to-change, double-click to reset, **keyframe diamond** (add/remove keyframe at playhead, arrows to jump to prev/next keyframe), and a **reset** icon per section.

### 9.1 Video / Image clip — tabs: Video · Audio · Speed · Animation · Adjust · Tracking
**Video → Basic**
- Transform: position X/Y, scale (uniform toggle, or width/height separately), rotation, anchor point, alignment buttons (left/centre/right, top/middle/bottom, fit/fill canvas), flip H/V.
- Opacity, **blend mode** (normal, darken, multiply, color burn, linear burn, lighten, screen, color dodge, add, overlay, soft light, hard light, difference, exclusion, hue, saturation, color, luminosity).
- Crop (with values), rounded corners, drop shadow, stroke/border.
- **Stabilize** (non-AI: feature-point/optical-flow based; levels Low/Recommended/Max) — later phase.
- Canvas background (per clip override).
- Motion blur (from keyframed motion).

**Video → Remove BG** (non-AI part only): **Chroma key** — color picker/eyedropper, intensity, shadow, edge feather, spill suppression. (AI auto-removal and custom removal are excluded.)

**Video → Mask**: none, linear (split), mirror (filmstrip), circle, rectangle (with round corners), heart, star, **pen/custom** mask. Position, rotation, size, feather, invert; multiple masks per clip; all keyframable; masks editable directly on canvas.

**Audio tab** (for clips with sound): volume (dB, −∞ to +20), fade in/out, normalize loudness, noise reduction (spectral/gate DSP), voice effects (Section 9.5), audio channel (stereo/left/right/mono).

**Speed tab**
- **Standard:** 0.1× to 100×, quick buttons (0.5×, 1×, 2×, 3×), resulting duration shown and editable, **keep pitch**, **smooth slow-mo** (frame blending; optical-flow interpolation later).
- **Curve:** presets — Custom, Montage, Hero, Bullet, Jump Cut, Flash In, Flash Out — and a **speed curve editor** (draggable points), keep pitch.
- **Reverse** and **Freeze frame** buttons.

**Animation tab**: **In**, **Out**, **Combo** (loop) presets with duration slider. In/Out: fade, zoom in, zoom out, slide L/R/U/D, rotate, spin, swing, pop/bounce, blur in/out, wipe, shake, flip, 3D turn. Combo: wobble, pulse, rotate loop, float, shake loop, zoom in-out loop. Choosing a Combo removes In/Out (as in CapCut). Hover preview on presets.

**Adjust tab**
- **Basic:** temperature, tint, saturation, vibrance, exposure, brightness, contrast, highlights, shadows, whites, blacks, brilliance, sharpen, clarity, fade, vignette, grain.
- **HSL:** 8 color channels (red, orange, yellow, green, cyan, blue, purple, magenta) × hue/saturation/lightness.
- **Curves:** RGB master + R/G/B curves with draggable points.
- **Color wheels:** lift/gamma/gain/offset (shadows/midtones/highlights/global).
- **LUT:** load .cube, strength.
- Scopes panel (optional window): waveform, vectorscope, histogram.
- "Save as preset" and "Apply to all".

**Tracking tab** (non-AI): point/planar motion tracking by template matching/optical flow; attach text/sticker/shape/mask to the tracked point — later phase.

### 9.2 Text / Caption clip — tabs: Text · Animation · Tracking
- Text box: content (multi-line), **font**, size, **bold/italic/underline**, case (Aa/AA/aa), color/gradient fill, character spacing, line spacing, alignment (L/C/R/justify, top/middle/bottom), auto-wrap width, preset styles.
- **Stroke** (up to 3 strokes, color, width), **background** (color, opacity, padding, corner radius), **shadow** (color, opacity, blur, distance, angle), **glow**, **curved text** (arc amount), **bubble** shapes behind text.
- Per-word / per-character styling inside one text (select words in the editor → change color/size/font) — needed for "Red Keyword" style captions.
- Transform/opacity/blend mode like video.
- **Animation:** In/Out/**Loop** presets incl. typewriter, fade per character, pop per word, wave, slide up per line, blur in, glitch, bounce, flicker, highlight sweep, karaoke fill.
- **Text animators (motion graphics):** per-character/word/line ranges with offset, delay, easing — animate position, scale, rotation, opacity, color, blur.
- **Captions only:** "apply to all captions" checkbox, style templates, words-per-caption setting, max line length, position preset.
- Sinhala, Tamil and English must render correctly in every style (shaping, stroke around conjuncts, per-character animation respecting grapheme clusters — never split a Sinhala cluster).

### 9.3 Shape / vector clip — tabs: Vector · Animation · Tracking
- Size W/H (lock ratio), scale width/height, uniform scale, position, rotation, alignment, opacity, blend mode.
- **Fill:** on/off; **solid**, **linear gradient**, **radial gradient** (multiple stops, angle), **texture/image fill**.
- **Color picker** (as CapCut): saturation/value square, hue bar, eyedropper (pick from player), hex input, opacity %, gradient angle, recent and saved swatches.
- **Stroke:** color/gradient, width, inside/centre/outside, dash pattern, line caps/joins.
- Corner radius, polygon sides, star points/inner radius.
- **Pen tool editing:** add/move/delete nodes, bezier handles, close path, convert corner/smooth.
- **Trim paths** (start/end/offset — for line-drawing animations), all keyframable.

### 9.4 Sticker clip
Transform, opacity, blend mode, animation (In/Out/Combo), speed for animated stickers, loop, tracking.

### 9.5 Audio clip — tabs: Basic · Voice effects · Speed
- Volume (with keyframes = volume automation drawn on the clip), fade in/out handles on the clip, pan.
- **Normalize loudness** (target −14 LUFS for social, adjustable).
- **Noise reduction**, **EQ** (presets + 5-band), **compressor/limiter**, **de-esser** (simple).
- **Voice effects** (DSP, no AI): pitch up/down (chipmunk, deep), robot, echo, reverb (room/hall), telephone, megaphone, radio, underwater, vibrato.
- **Auto ducking:** lower music under voice tracks (sidechain envelope), amount and attack/release.
- Speed with keep pitch; beat detection (Section 8.3).

### 9.6 Effect / Filter / Adjustment clips
Parameters of the effect/filter, strength, keyframes, "apply to" target, duration on the timeline.

### 9.7 Transition (selected on timeline)
Duration, easing, direction, preset-specific options, "Apply to all".

---

## 10. Keyframes & motion graphics (the "Premiere Pro" part)

- **Every** numeric/color property can be keyframed (transform, opacity, crop, mask, color adjustments, effect params, volume, text properties, shape properties, trim paths).
- Keyframe diamonds on the clip in the timeline; drag to retime; multi-select keyframes; copy/paste keyframes; delete.
- **Easing:** linear, hold, ease in, ease out, ease in-out, and named presets (sine, quad, cubic, back, elastic, bounce) + **custom bezier** per keyframe.
- **Graph editor** panel (toggle under the timeline or as an inspector section): value graph and speed graph per property, bezier handles, zoom/pan — like Premiere/After Effects.
- **Motion path** shown on the canvas for keyframed position; drag path points and bezier handles on the canvas.
- **Parenting**: link a layer to another layer so it follows its transform (e.g. text follows a shape).
- **Null/controller layer** (invisible transform layer) for rigging groups.
- **Animation presets library**: save any set of keyframes/animators as a reusable preset ("My animations"); our built-in In/Out/Combo animations are themselves defined as presets in `vz-presets` (data, not hard-coded).
- **Motion blur** based on movement between frames (sample count setting).
- Wiggle/random motion generator (seeded noise on any property) — a practical non-AI "expression".

---

## 11. Export, settings, performance, extensibility

### 11.1 Export dialog
- Title, save location, **Video**: resolution (480p, 720p, 1080p, 2K, 4K, custom; keeps aspect), bitrate (Lower / Recommended / Higher / Custom kbps), codec (H.264, HEVC; ProRes on mac via VideoToolbox if available), format (MP4, MOV), frame rate (24, 25, 30, 50, 60, or project), color (Rec.709 SDR).
- **Hardware encoding** when available (NVENC / QSV / AMF / VideoToolbox) with software fallback (choose an LGPL-compatible encoder, e.g. OpenH264, or ask the owner about x264 licensing).
- **Audio only** export: MP3, WAV, AAC, FLAC.
- **GIF** export (resolution, fps, dithering).
- **Captions** export: SRT / TXT.
- **Current frame** as PNG/JPG (from the player too).
- Export **range** (in/out or selected clips), estimated file size, progress with time remaining, background export (keep editing), cancel, "Open folder" when done.
- Export presets: YouTube 1080p, YouTube 4K, TikTok/Reels/Shorts 9:16 1080×1920, Facebook feed, Instagram 1:1/4:5, WhatsApp-friendly small file.

### 11.2 Settings
- General: language (English now; UI strings in a translation file so **Sinhala UI** can be added), autosave interval, default project fps/resolution, default image duration, default transition duration.
- Storage: project folder, cache folder + size + **Clear cache**, proxy location.
- Performance: hardware decode/encode on/off, preview quality default, proxy auto-create for >1080p, max RAM for frame cache.
- **Keyboard shortcuts** editor with presets "vidzcut (CapCut-style)" and "Premiere-style", conflict detection, reset.
- Appearance: dark theme (light theme later), accent color, UI scale.

### 11.3 Performance features
- **Proxies** (auto/manual) for heavy media; toggle in player.
- **Render cache** for heavy sections (shown as a coloured bar on the ruler).
- Thumbnail and waveform caches on disk (keyed by file hash + params).
- Lazy loading of asset grids; never decode more than needed.

### 11.4 Extensibility (for future AI features)
- `FeatureProvider` trait with capability flags (e.g. `CaptionGenerator`, `BackgroundRemover`, `TextToSpeech`). The UI shows a feature only if a provider is registered. Ship with **none** registered. The prototype's auto-caption work can become a provider later.

### Default keyboard shortcuts (Windows; use Cmd instead of Ctrl on macOS)
Space play/pause · J/K/L shuttle · ←/→ frame step · Home/End · I/O mark in/out · Ctrl+B split · Q / W split-delete left/right · Delete / Shift+Delete ripple delete · A select tool · B blade tool · Ctrl+Z / Ctrl+Shift+Z · Ctrl+C/V/X/D · Ctrl+Alt+V paste attributes · Ctrl+G group / Ctrl+Shift+G ungroup · Alt+G compound clip · M marker · N snapping · Ctrl+= / Ctrl+- zoom · Shift+Z fit · Ctrl+E export · Ctrl+S save · Ctrl+I import · F full-screen preview.

---

## 12. Phases (build in this order; stop after each one for the owner to test)

Each phase lists **Done when** criteria. Write them into `ROADMAP.md` as checkboxes.

**Phase 0 — Study & foundation**
Read the prototype (Section 1), write `docs/PROTOTYPE_NOTES.md`, `docs/ARCHITECTURE.md`, `ROADMAP.md`, `CLAUDE.md`, `docs/BUILDING.md` (how to build on Windows and macOS, including FFmpeg setup — make it automatic via a script if possible). Create the workspace and an app window with the full empty layout (top bar, asset tabs, player, inspector, timeline) using the vidzcut logo and theme. Home screen with Create project.
*Done when:* the owner can double-click/run one command and see the vidzcut window on Windows (and the build instructions for macOS work).

**Phase 1 — Minimum real editor**
Import media, media grid with thumbnails, drag onto main track, playback with synced audio, scrub, split/trim/delete/ripple, undo/redo, save/open project, autosave + recovery, export 1080p H.264 MP4.
*Done when:* the owner can cut a 2-minute video into pieces, reorder them, save, reopen, and export a playable MP4 identical to the preview.

**Phase 2 — Multi-track compositing & text**
Overlay tracks, images, transform handles on canvas, smart guides, opacity, blend modes, crop, canvas aspect ratios & backgrounds, audio tracks with waveforms, volume/fades, snapping, magnet, linkage, multi-select, copy/paste, markers. Text clips with full basic styling and correct **Sinhala** shaping. Stickers (import + a few built-in).
*Done when:* the owner can make a 9:16 short with a talking-head video, a logo overlay, a Sinhala headline and background music, and export it.

**Phase 3 — Animation**
Keyframes on all properties, easing presets + custom bezier, keyframe navigation, In/Out/Combo animation presets, transitions library (first 15+), speed (standard, curve presets + editor, keep pitch, reverse, freeze frame).
*Done when:* a title can fly in with ease-out, a clip can zoom slowly with keyframes, and cuts can use transitions.

**Phase 4 — Color & effects**
Adjust (basic, HSL, curves, wheels, LUT), filters library + .cube import, adjustment layers, effects library (first 25+), masks (all shapes + pen), chroma key, scopes.

**Phase 5 — Vector, advanced text & captions**
Shapes + pen tool, gradients, color picker with eyedropper, stroke/trim paths, advanced text (multiple strokes, shadow, glow, curved, per-word styling, text animators), text templates, captions (manual, import/export SRT/VTT/LRC/ASS/TXT, caption list editor, the prototype's caption styles, apply-to-all), compound clips and multiple timeline tabs.

**Phase 6 — Pro audio**
Loudness normalization, noise reduction, EQ, compressor, voice effects, auto ducking, beat detection, voiceover recording, audio-only export.

**Phase 7 — Export & polish**
All export options (Section 11.1), hardware encoders, proxies, render cache, settings, shortcut editor, templates (save/use), home screen polish, error dialogs, installers for Windows and macOS (`cargo-packager`), first-run sample project.

**Phase 8 — Motion graphics extras**
Graph editor, motion paths on canvas, parenting/null layers, motion blur, wiggle, animation preset saving, motion tracking and stabilization (non-AI).

---

## 13. Testing requirements

- Unit tests in `vz-core` for time math, every command's apply/revert, ripple/roll/slip/slide logic, keyframe interpolation and easing, project save → load round-trip, migrations.
- Golden-image tests in `vz-render`: render known frames of sample projects and compare with stored PNGs (with tolerance) — covers effects, transitions, blend modes, text (including a Sinhala sample).
- Export test: render a 5-second sample project and check duration, resolution, fps, and A/V sync with `ffprobe`.
- Sample media in `assets/test-media/` (generated by us: color bars, tone, counting numbers) — never commit large files.
- `docs/TESTING.md` with the owner's manual checklist per phase, written in simple steps ("Click Import → pick any MP4 → you should see a thumbnail").
- Run the app on **both** Windows and macOS before calling a phase done; if you cannot run macOS, set up a GitHub Actions build for macOS and Windows and tell the owner.

---

*End of spec. Start with Phase 0. Plan first, explain the plan simply, and wait for the owner's OK before coding.*

---

## Spec changes (owner decisions after 2026-10-01)

- 2026-10-02 (owner, Phase 0): tools may be installed (Rust, MSVC Build Tools, LLVM, LGPL FFmpeg). UI toolkit stays **egui**, after a Sinhala spike (fallback options: iced / Slint). Software H.264 fallback = **OpenH264** (licence details to be re-checked and confirmed before release). Gemini auto-captions stay **out of v1** (plugin later). Assistant replies in **simple English**. Project extension stays `.vzp`; a `.vidz` importer comes later.
