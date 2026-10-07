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

## Current state (2026-10-08) — Phase 6 and Phase 7a-7c built (gate passed), waiting for the owner's hand test; Phase 5 hand test was never reported back
### Phase 7 (7a-7c) additions (read this first)
- Export (`vz-export`): `ExportSettings` now has `codec` (`VideoCodec` H264 / Hevc), `container` (Mp4 / Mov), `custom_kbps`, `use_hardware`; `encoder_candidates(codec, hardware)` (H.264: NVENC, QSV, AMF, OpenH264; HEVC: NVENC, QSV, AMF, libkvazaar software), HEVC is tagged `hvc1`. New files: `gif.rs` (`export_gif`, NeuQuant palette shared by all pictures + none / ordered / Floyd-Steinberg dithering, `gif` + `color_quant` crates), `picture.rs` (`save_picture` PNG / JPG, `image` jpeg feature), `presets.rs` (7 social presets, `Preset::fits_canvas`), `audio.rs` (Phase 6).
- Export window: `vz-ui/src/export_dialog.rs` (`ExportContext` = playhead, selected clips' span, caption lines; modes Video / Sound only / GIF / Picture / Captions; picture and captions finish at once, the rest run in a thread; "Keep editing" = `tucked_away`). `Action::ExportPicture` from the player's "Save picture" button.
- Settings: `settings.rs` has `Preferences` (autosave_secs, ui_scale, hardware_encoder), `shortcut_preset`, `shortcuts` (overrides: command id → "Ctrl+Shift+Z"). `settings_window.rs` (pages General / Appearance / Performance / Storage / Shortcuts, `SettingsState` lives in `EditorState.settings_ui`). `keymap.rs`: `Cmd` (20 commands), `Combo`, `Preset` (Vidzcut, Premiere), `Keymap::from_settings`; `editor/mod.rs::handle_keys` loops over `in_matching_order()` (more modifiers first) and calls `run_command`. Fixed (not editable): Ctrl+Y redo, Backspace delete, Ctrl+C/X/V, Alt+arrows nudge. UI scale is applied with `ctx.set_zoom_factor` at the top of `App::ui` (**screen-script coordinates assume scale 100 %**).
- Templates: `vz-core/src/user_template.rs` (`UserTemplate`: parts = text / shape / adjustment clips only, start relative, canvas size remembered, `build(w, h)` scales positions via `Animatable::scale_values`), JSON files in `<data dir>/templates/`, panel `editor/user_template_panel.rs` (tile previews drawn with the real renderer; right-click = delete).
- App: `Dialog::Problem` + `VidzApp::problem(title, plain, details)` for failed open / save / restore; home screen actions `CreateWithAspect`, `OpenSample` (`open_sample`: four starter templates on a dark blue canvas), `OpenSettings`.
- Tests added: vz-export 20 (HEVC in MOV with custom bit rate + 30 fps, GIF for each dither mode + cancel, PNG / JPG, presets), vz-ui keymap 4, vz-core user_template 3. Checked in the real window: export pages, Settings pages, rebinding + clash message, My templates empty state, sample project, Help menu. **Not checked: saving / using a template end to end by hand, the Problem window, recording a real hardware-encoder export of HEVC.**
- Not done (7d, 7e): proxies, hardware decode, render cache, installers (`cargo-packager`; needs owner decisions on signing), Sinhala interface strings, accent colour.

### Phase 6 additions (read this first)
- Model: `vz-core/src/audio_fx.rs` (`AudioFx` on every `Clip.audio`, all off by default and `#[serde(default)]`, so no project-format change: pan, `ChannelMode`, `Normalize{target,measured}`, `Denoise{amount, profile}`, `Eq` (5 bands, `EQ_PRESETS`), `Compressor`, `deess`, `Voice{effect,amount}` (11 `VoiceEffect`), `Duck`, `beats` in media-file time). `Clip::beat_times()` (`speed.rs`) maps beats to the timeline; the timeline snaps to them and draws ticks.
- DSP: `vz-audio/src/fx/` (`biquad`, `eq`, `dynamics` = compressor / limiter / de-esser, `voice`, `fft`, `denoise` = `NoiseMeter` + `Denoiser`, 512-sample frames, 11 ms late, `loudness` = BS.1770 K-weighting + gates, `beats`, `mod.rs` = `Chain` in the order denoise → EQ → de-esser → compressor → voice). Everything works on interleaved stereo `f32`.
- Mixer (`mixer.rs`): per-clip `ClipState` (chain, expected next sample; a seek or a settings change rebuilds the chain), `clip_sound()` = read → chain → `apply_gain` (channel, balance, normalise factor, volume, fades). Ducking = two passes: clips without `duck` first, then each ducked clip is lowered by the envelope of that first mix. Master `soft_limit` replaces the old hard clamp.
- Measuring: `vz-audio/src/analysis.rs` (`analyse(path, from, length, Wanted{loudness,noise,beats})`), run by `vz-ui/src/audio_jobs.rs` in background threads (results are written into the clip as one undo step each; the same module extracts audio to WAV). Noise profiles are measured at 48 kHz and re-mapped for other mix rates (`profile_for_rate`).
- Other audio files: `wav.rs` (16-bit WAV writer), `sfx.rs` (9 synthesised sound effects, no licences), `recorder.rs` (cpal microphone → mono WAV, `input_devices()`), `preview.rs` (listen to a file).
- Export: `vz-export/src/audio.rs` (`export_audio`, `AudioFormat` Wav / Flac / Aac(m4a) / Mp3 via LAME, chunk = encoder frame size). The export dialog has Video / Audio only.
- UI: `inspector/audio.rs` (Audio tab pages Basic / Clean-up / Voice effects; `EditorState.audio_page`), `editor/audio_panel.rs` (asset tab Audio: Music and Sound effects from user folders saved in `UiSettings.music_dirs / effect_dirs`, built-in effects, Extract audio, Voiceover with microphone choice, level meter, 3-2-1 countdown, play-along option). Files are kept in `<config or data dir>/audio/{effects,extracted,recordings}`.
- Tests added: vz-audio 61 total (fx unit tests, `fx_tests.rs` through the real mixer), vz-core `audio_fx`, vz-export audio export (WAV/FLAC/AAC/MP3 + cancel). Checked in the real window: Audio tab pages, loudness measuring (−21.8 LUFS for the 440 Hz test clip), sound-effect list, extract audio, voice-over countdown. **Not checked by a person: real microphone recording, listening to the effects.**
- Not done (see ROADMAP): noise / loudness for compound clips, retimed compound effects, volume automation drawn on the clip, J/L cuts ("detach audio"), latency compensation while recording, pitch-shift quality is "voice effect" grade (granular), de-esser is a broadband gain.


### Phase 5 additions (read this first)
- Shapes: `vz-core/src/content.rs` (`ShapeKind` 16 kinds, `ShapeContent` gradient / `StrokePlace` / dash / `LineCap` / `LineJoin` / sides / points / inner / trim_* / `path: Option<PenPath>`; `PenNode` = point + in/out handles as offsets in shares of the box). Drawing in `vz-vector` (tiny-skia: gradients, stroke inside/outside via `Mask`, dashes, own trim-path code on a flattened path).
- Pen tool: `vz-ui/src/editor/pen.rs` (`PenSession` in `EditorState.pen`, points kept in canvas pixels while editing, `fit()` stores them as shares of a box fitted round the curve, `to_canvas()` the other way, `split`, `segment_at`, `toggle_smooth`; editing a path bakes the clip's scale / turn / position into the points). Started from Stickers → Shapes → "Pen tool" or Shape tab → "Edit points". `canvas.rs` hands the mouse to `pen::show` while a session is open; `pen::keys` is called first in `handle_keys`.
- Text: `vz-text` rich engine (spans, gradients, up to 3 outlines, glow, curve, case, word modes, animators). Captions: `vz-core/src/captions.rs` (SRT/VTT/LRC/ASS/TXT), `document/caption_ops.rs`, `vz-presets/src/captions.rs` (21 styles), `editor/caption_panel.rs`. Colour picker + eyedropper: `editor/widgets.rs`.
- Templates and groups: `vz-presets/src/templates.rs` (9 templates, built for the canvas size), `Document::add_group` (every part on its own new track, one undo step), `group_clips / ungroup_clips / group_mates` (`document/group.rs`); clicking a grouped clip selects its group (Alt+click = one), Ctrl+G / Ctrl+Shift+G, panel `editor/template_panel.rs` (tiles are rendered with the real renderer).
- Compound clips and timelines: `Content::Compound(TimelineId)`; `Project.active: Option<TimelineId>` decides what `timeline()` means; new commands `AddTimeline / RemoveTimeline / RenameTimeline`; **history entries remember which timeline they were made in and undo/redo switch back to it**; `document/timelines.rs` (`add/duplicate/rename/delete_timeline`, `set_active_timeline`, `make_compound`, `Timeline::copied_as`). Renderer: `render_timeline(project, timeline, t, size, backdrop, depth)`; a compound is drawn twice (on black and on white) and "unmatted" to get a see-through picture (`unmatte`), depth limit 6; compounds are never pasted (`is_plain`). Mixer: `Mixer::mix` recurses into compound clips (speed 1 only). UI: timeline tabs (`timeline.rs::tabs`), toolbar Compound / Open, Alt+G, double-click opens, `Ctx::open_timeline`. Export always exports the **open** timeline.
- Dev script has `dclick:x,y` now. Tests added: vz-vector 15, pen 10, group 4, timelines 8, templates 4, render compound 8 / captions 4, export 1, audio 1.
- Not done (see ROADMAP): SVG import (MPL resvg), pen masks, keyframes for shape values, break apart compound, retimed compound sound, user templates, user shape presets.

### Phase 4 additions
- Model: `vz-core/src/grade.rs` (`Adjust` incl. HSL / curves / wheels / `LutRef`, `MaskDef`, `ChromaKey`, `EffectRef`); `Clip` has `adjust`, `effects`, `masks`, `chroma`; `Content::Adjustment` (an adjustment layer; a lone effect on the timeline is the same thing with one `EffectRef`) lives on the new `TrackKind::Effect`; `Document::add_adjustment_auto`, `update_clips` (one undo step for many clips).
- Render order per layer (`vz-render/src/lib.rs`, `process`): chroma key → colour (`color.rs`) → effects (`effects.rs`, one routine per `vz_presets::effects::Kind`) → masks (`keying.rs`) → animation blur → composite. An adjustment layer (`draw_pass`) runs colour + effects on a copy of the canvas and blends it back through its masks and opacity. Keyed / masked pictures are blended, never `paste_centered` (see `is_plain`). LUTs: `lut.rs` (`.cube` parser, 4 builtin looks, `LutCache`). Scopes: `scopes.rs`.
- Data: `vz-presets/src/{effects,filters}.rs` (39 effects with control specs, 36 filters as `Adjust` presets).
- UI: asset tabs Filters / Effects / Adjustment in `look_panel.rs` (tiles are rendered with the real code on a sample picture; effects animate on hover); inspector tabs `Adjust` (`inspector/adjust.rs`: basic/HSL/curves/wheels/look) and `Effects` (`inspector/looks.rs`: effect list, mask and chroma sections also used by the Video tab); `scopes_window.rs` (button in the player title row).
- Speeds measured in release at 1080p: most effects 3-20 ms, blurs 45-120 ms (`cargo test --release -p vz-render effects_on_a_1080p -- --ignored --nocapture`).
- Not done (see ROADMAP): on-picture mask handles / pen mask, keyframes for look values, user presets, copy-paste attributes, frame effects, GPU compositor.

### Phase 3 additions

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
- The Templates tab (Phase 7) is a placeholder ("Coming in Phase 7"); the Library / Subprojects sub-menus too. Variable fonts are used at their default weight in the interface (video text selects bold properly).

## Next step
Owner runs the **Phase 6** and **Phase 7** checklists in `docs/TESTING.md` and says OK or reports problems → then **7d** (proxies, hardware decode, render cache) and **7e** (installers, ask the owner about code signing first). Phase 8 after that.
