# vidzcut (Rust rebuild) — hand-off for Claude Code

**Read `docs/SPEC.md` first (the owner's master spec), then this file.** Where they disagree, SPEC wins; tell the owner.

## Who the owner is / how to work
- Owner is **not a programmer**. Plain, simple English. No jargon without a one-line explanation. Never ask them to edit code.
- Decisions: give 2–3 options + a recommendation, then wait.
- Work in **phases** (ROADMAP.md). At the end of a phase: what was built, click-by-click test steps, known issues, next phase — then **stop and wait for OK**.
- Every stop: `cargo fmt --all`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, `cargo build --release` pass; update ROADMAP.md + this file; commit.
- Ask before: paid services, network/cloud features, GPL dependencies, breaking project-file changes, any download/install.
- Never leave the app broken. Unfinished = hidden or "Coming soon".

## Hard rules
- **CapCut is a reference for layout/behaviour only.** Never copy/read/extract anything from a CapCut install or its data (`C:\Users\bishw\AppData\Local\CapCut\Apps`, `F:\All Capcut` were attached by the owner — **ignore them, not needed**). No CapCut name/logo/assets anywhere.
- **No AI / cloud features** in this version (spec §0). Keep the `FeatureProvider` hook (spec §11.4).
- **Licences:** MIT/Apache/BSD/OFL/CC0 only. FFmpeg = LGPL, dynamically linked. No GPL (x264, x265, Rubber Band, GPL FFmpeg) without asking. Log everything in `THIRD_PARTY_LICENSES.md`.
- **Sinhala is first-class.** Test Sinhala in every text-related step ("ශ්‍රී ලංකාව", "ක්‍රීඩාව", "ඇස් සම්බන්ධ රෝග"). Never split a grapheme cluster.
- Time = `i64` microseconds. Edits go through commands (undo/redo). Preview and export share one render path.
- Brand: black UI, yellow accent `#FCC610` (hover `#FFD84D`, dark `#2E2606`, text-on-accent `#0A0A0A`, bg `#0A0A0A`, panel `#151515`, panel2 `#202020`). Logo files in `assets/brand/`.

## Where things are
- `D:\Projects\vidzcutpro` — this repo (new Rust project).
- `D:\Projects\vidzcut` — the old Python/Qt prototype v1.4.0 (**reference only**, don't copy architecture). Notes: `docs/PROTOTYPE_NOTES.md`. Real test video: `D:/Gloma/with track.mp4` (read-only).
- Docs: `docs/SPEC.md`, `docs/ARCHITECTURE.md`, `docs/PROTOTYPE_NOTES.md`, `docs/BUILDING.md`, `ROADMAP.md`.

## Current state (2026-10-03) — Phase 3 built, waiting for the owner's hand test (Phase 1 and 2 hand tests: owner said "OK, continue" after Phase 2)

### Phase 3 additions (read this first)
- `vz-anim`: `Easing` now has `Named(Family, Dir)` (sine/quad/cubic/back/elastic/bounce) and `Bezier(..)`; `Animatable` has `set_key/remove_key/key_at/prev_key/next_key/set_easing/move_key/shift/freeze_at`. `shift` keeps the value at the new start (used when clips are cut or trimmed: `Clip::shift_keys`, which also shifts `speed`).
- UI keyframes: `editor/keyframes.rs` (`Prop`, `write` = fixed value or keyframe at the playhead, `toggle_key`, `neighbour`); diamonds and `anim_slider` in `inspector.rs`; canvas drags and nudges write through `keyframes::write`.
- Animations: `vz-presets/src/animation.rs` (data + `state_at(clip, local)`), `Clip.anim_in/out/combo: Option<AnimRef>`, applied in `Renderer::layer_for`; Animation tab `inspector/animation.rs`.
- Transitions: `Track.transitions: Vec<Transition{after, preset, duration, easing}>`; `Timeline::layers_at` returns a `Layer` with `transition: Some(LayerTransition)` while one runs (centred on the cut); recipes in `vz-presets/src/transition.rs`, pixels in `vz-render/src/transition.rs`; `Document::set_transition / apply_transition_to_all`; UI `transition_panel.rs` + `inspector/transition.rs`; badges drawn/clicked in `timeline/{draw,input,layout}.rs`; `EditorState.selected_transition`. Renderer readers are keyed by (file, track, incoming?) so a transition between clips of one file does not thrash the decoder.
- Speed: `Clip.speed: Animatable<f32>`, `reverse`, `keep_pitch`; maths in `vz-core/src/speed.rs` (`advance`, `source_span`, `source_time`, `split_sources`, `trimmed_left`, `with_duration`, `room_before`, `max_duration`); edits in `document/retime.rs` (`set_speed`, `set_speed_curve`, `set_reverse`, `freeze_frame`); trims/splits in `edit.rs`/`edit/free.rs` call the `Clip` helpers (`trim_left` now takes `media_len`). Audio: `vz-audio/src/retime.rs` (`Retimer`: varispeed or WSOLA, reverse via a virtual stream), used by `Mixer`. Video reverse: `VideoDecoder` keeps a run of decoded pictures when asked for earlier times (`back`). UI: `inspector/speed.rs` (Standard / Curve tabs, curve editor), toolbar Freeze.
- Not done (see ROADMAP): graph editor, bezier handles UI, motion path, copy/paste keyframes, text animators, audio crossfade at transitions, frame-blended slow motion.

### Phase 2 additions

### Phase 2 additions (read this first)
- Model (`vz-core`): `Clip` has `content: Content::{Media, Text, Shape}`, `transform` (animatable props via `vz-anim`), `opacity`, `blend`, `crop`, `volume`, fades. Tracks: Main (magnetic), Overlay, Text, Audio (free: no overlaps, no ripple). Project FORMAT_VERSION 2 (v1 migrates). New `Document` calls: `add_text_auto`, `add_shape_auto`, `add_media_auto`, `place_*`, `update_clip(id,label,merge,f)`, `copy_clips/paste_clips/duplicate_clips` (`document/clipboard.rs`), `toggle_marker`, `split_clips`, `cut_clips_at`, `move_clip_to_new_track`.
- Rendering: `vz-render` composites on the CPU (rayon) in `composite.rs`/`blend.rs`; text comes from `vz-text` (`TextEngine`, cosmic-text, bundled fonts, outline via distance transform, blurred shadow, background box), shapes from `vz-vector` (`ShapeCache`, tiny-skia). Text/shape sizes are in *reference pixels* (short canvas side = 1080) via `vz_text::unit`. Preview and export share `Renderer::render` (test `text_shapes_and_overlays_are_in_the_exported_file`).
- Audio: `vz-audio::Mixer` applies volume/fades/mute; `waveform.rs` measures peaks (50/s) in a background thread (`vz-ui/src/waveforms.rs`).
- UI: `editor/canvas.rs` (click/drag/scale/rotate handles, snap guides, nudge), `geometry.rs` (placed/pick; thread-local `with_text`), `inspector.rs` + `inspector/{text,shape}.rs`, `text_panel.rs`, `sticker_panel.rs`, timeline split into `timeline/{layout,draw,input,tests}.rs`. Keys: Ctrl+C/X/V/D, M, Alt+arrows nudge (Shift = 10 px).
- Bundled: `assets/fonts` (Inter, Noto Sans/Serif Sinhala, Noto Sans Tamil, OFL), `assets/icons/lucide.ttf` (ISC; codepoints in `theme::icons`). Python heredocs with Windows paths or apostrophes break in Bash: write scripts to a file with the Write tool.
- Not done in Phase 2 (see ROADMAP): typing directly on the canvas, SVG stickers (needs OK for resvg MPL-2.0), video/audio linkage behaviour, safe-zone/grid buttons, GPU compositor.

- Quality gate: `powershell -ExecutionPolicy Bypass -File scripts\gate.ps1 [-Release]` (fmt, clippy -D warnings, all tests, optional release build). **Run it and only commit when it prints GATE PASSED** (a plain `;` chain once let failing lint through). ~110 tests.
- Crates with real code: `vz-core` (time, ids, project, media assets, timeline, `edit.rs` pure main-track ops, `command.rs` commands + history, `document.rs` = project + undo + save/open/autosave/recovery), `vz-media` (probe, frame-accurate `VideoDecoder`, `FrameSource`, background `MediaWorker` for import + thumbnails, `testutil` fixtures made with ffmpeg), `vz-render` (CPU `Renderer::render(project, t, size)` used by player AND export), `vz-audio` (`AudioReader`, `Mixer`, cpal `AudioEngine`: sound-card clock is master), `vz-export` (`export()` with encoder fallback NVENC→QSV→AMF→OpenH264, AAC, progress/cancel), `vz-ui` (app, home, editor/{topbar,assets,player,inspector,timeline}, preview thread, export dialog, settings, theme, dev hooks), `vidzcut` (main + logging). (Phase 2 filled vz-anim, vz-text, vz-vector, vz-presets.)
- Data flow: UI never edits the project directly; it calls `Document` methods (each is a `Command`). Drags use merge keys (`EditorState::new_merge_key`) and `doc.seal_history()` at drag end. Main track is magnetic (`edit::pack`).
- Preview: `Preview` thread (`vz-ui/src/preview.rs`) renders latest-wins at ≤960 px; playback clock = `AudioEngine::position()`, or wall clock when no sound device (`VIDZCUT_NO_AUDIO=1`).
- The project adopts the first video/image clip's shape and the nearest standard frame rate once (`Project::auto_settings`, one undo step); manual changes switch it off.
- UI facts: egui/eframe **0.36** (`App::ui`, `raw_input_hook`, `Panel::..show(ui,..)`, no `RawInput::modifiers`, `DroppedFile::path()` is a method). cosmic-text **0.19**, cpal **0.18** (`SampleRate = u32`), ffmpeg-next **9** (legacy `sws_scale` path: set colour matrix with `sws_setColorspaceDetails`).
- Files: settings `%APPDATA%\vidzcut\config\ui_settings.json`, autosave/recovery `…\config\recovery\<id>.autosave.json`, logs `%LOCALAPPDATA%\vidzcut\data\logs\vidzcut.log`, test fixtures `%TEMP%\vidzcut-fixtures`.
- **Developer switches** (docs in `crates/vz-ui/src/dev.rs`): `VIDZCUT_SCRIPT="wait:150;click:x,y;drag:x1,y1,x2,y2;key:Ctrl+B;shot:path.png;quit"` drives the real window (positions are egui points = screenshot pixels / 1.25 on the owner's PC); `VIDZCUT_IMPORT`, `VIDZCUT_PLAY`, `VIDZCUT_AUTOEXPORT`, `VIDZCUT_NO_AUDIO`, `VIDZCUT_CONFIG_DIR`, `VIDZCUT_LOG_DIR`. **Always set both DIR variables** so the real settings/recovery are untouched. A screenshot step blocks later steps until saved.
- Measured on this PC (i5-13420H, Intel QSV; NVENC unusable: driver 592 < 610 required by the FFmpeg build): 1080p/24 source plays at 30 pictures/s; 10 s 1080p export took 10.8 s (28 fps).
- Real test video (read-only): `D:/Gloma/with track.mp4`; run `VIDZCUT_REAL_VIDEO=... cargo test --release -p vz-export real_footage -- --ignored --nocapture`.
- Tool tips: the Bash tool breaks on heredocs containing apostrophes (Rust lifetimes) → write files with the Write tool, or put Python in a `/tmp/x.py` file first. After `cargo fmt` many `Edit`/`replace` strings no longer match (line wrapping): re-read the file first.
- Not done / unverified: **Sinhala typing + Backspace + cursor in egui's edit boxes** (owner to test); macOS build; GitHub Actions never ran (no repo); audio plays through the real device but nobody has *listened* yet (clock and mixing are tested); sound/picture offset from device latency is not compensated (tens of ms); setup-script "install" branches only run on a machine that already has everything.

## Decisions made by the owner (2026-10-02)
1. Tools may be installed: Rust, MSVC Build Tools, LLVM, LGPL FFmpeg. ✔
2. UI toolkit: **egui**, after a Sinhala spike (step 0c); fallback iced/Slint only if the spike fails. ✔
3. Software H.264 fallback: **OpenH264** (re-check its licence before release; ask owner). ✔
4. Gemini auto captions: **out of v1**, plugin later. ✔
5. Project extension `.vzp`; `.vidz` importer later (my default, not objected to). ✔
6. Replies in **simple English**. ✔

## Toolchain on this PC (installed 2026-10-02)
- Rust 1.99 stable (MSVC), cargo at `%USERPROFILE%\.cargo\bin` (new shells need it on PATH).
- VS 2022 Build Tools (C++ workload) at `C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools`.
- LLVM (libclang) via winget — see Current state for whether it finished.
- FFmpeg n9.0 **LGPL shared** (BtbN build) in `third_party\ffmpeg` (git-ignored). Matches crate `ffmpeg-next 9.0.0`. Has h264/hevc/av1 nvenc/qsv/amf + libopenh264; no x264/x265 (good).

## Known issues
- Tabs Effects..Templates (except Text, Stickers) are placeholders ("Coming in Phase N"). Variable fonts are used at their default weight in the interface (video text selects bold properly).

## Next step
Owner runs the Phase 3 checklist in `docs/TESTING.md` and says OK or reports problems → **Phase 4** (colour and effects: adjust, filters/LUTs, effects library, masks, chroma key, scopes, adjustment layers; the GPU compositor probably arrives here because effects need it).
