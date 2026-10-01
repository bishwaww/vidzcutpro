# vidzcut — Architecture (plan, written in Phase 0)

Status: **PLAN — waiting for the owner's OK.** No Rust code exists yet.
Follows `docs/SPEC.md` (the master spec). Where I suggest a change, it is in section 6 with a reason.

## 1. The idea in plain words

Think of vidzcut as a small film studio with separate rooms. Each room is a Rust "crate" (a code package) that does one job and knows nothing about the others' insides:

```
                    ┌──────────────┐
                    │   vidzcut    │  the app you double-click (window, menus, glue)
                    └──────┬───────┘
                           │
                    ┌──────▼───────┐
                    │    vz-ui     │  every panel you see (egui)
                    └──────┬───────┘
      ┌──────────┬─────────┼──────────┬───────────┬───────────┐
┌─────▼────┐ ┌───▼─────┐ ┌─▼───────┐ ┌▼─────────┐ ┌▼─────────┐ ┌▼──────────┐
│ vz-render│ │vz-media │ │vz-audio │ │vz-export │ │ vz-text  │ │vz-vector  │
│ GPU draw │ │ decode  │ │ sound   │ │ final MP4│ │ fonts +  │ │ shapes,   │
│ + shaders│ │ thumbs  │ │ mixer   │ │          │ │ Sinhala  │ │ pen paths │
└─────┬────┘ └───┬─────┘ └─┬───────┘ └┬─────────┘ └┬─────────┘ └┬──────────┘
      └──────────┴─────────┼──────────┴────────────┴────────────┘
                    ┌──────▼───────┐   ┌────────────┐   ┌────────────┐
                    │   vz-core    ├──►│  vz-anim   │   │ vz-presets │
                    │ the project: │   │ keyframes, │   │ effects,   │
                    │ tracks,clips,│   │ easing     │   │ transitions│
                    │ undo, save   │   │            │   │ (data)     │
                    └──────────────┘   └────────────┘   └────────────┘
```

Rules that keep it healthy:
- **`vz-core` has no window, no GPU, no FFmpeg.** It is pure logic ("a clip starts at 3.2 s and lasts 5 s"), so it can be tested thousands of times a second.
- **The UI never changes the project directly.** It sends a *command* ("split clip X at 12.4 s"). Every command can be undone. This gives unlimited undo/redo and makes autosave simple.
- **Preview and export use the same drawing code.** The player and the Export button both ask: "what does frame T look like?" → one function answers. So what you see is what you export.
- **Time is whole numbers** (microseconds), never decimals, so clips never drift by a hair.
- **Everything slow runs in the background** (decoding, thumbnails, waveforms, export) and reports progress. The window must never freeze.

## 2. What happens when you press Play

1. **Audio thread** (cpal) plays the mixed sound. Its position is the **master clock**.
2. **Playback controller** asks every frame: "what time is it by the audio clock?"
3. `vz-core::evaluate(timeline, t)` returns the **layer stack**: which clips are visible at `t`, with their animated values (position, opacity, effects…) already worked out.
4. **Decoder workers** (vz-media) keep the next ~10–20 frames of each visible video ready (GPU-uploaded, LRU cache). If a frame is late, the player *drops* it (sound never stutters).
5. **Compositor** (vz-render, wgpu) draws layers bottom→top, running effect/transition/colour shaders, into one texture.
6. egui shows that texture directly in the Player panel (no copy back to the CPU).

Export is the same loop without the audio clock: for each frame number → evaluate → composite → read back → FFmpeg encoder; sound is mixed offline and muxed in.

## 3. The crates (what each one owns)

| Crate | Owns | Key third-party crates |
|---|---|---|
| `vz-core` | Project, MediaPool, Timeline, Track, Clip, Transition, `Animatable<T>`, commands + undo, time math, save/load + migrations, `FeatureProvider` trait | serde, serde_json, uuid, thiserror |
| `vz-anim` | Keyframes, easing (presets + bezier), interpolation, motion paths, wiggle | — (own code) |
| `vz-media` | Probe, decode (software first, hardware later), frame cache, thumbnails, waveforms, proxies, relink helpers | ffmpeg-next / ffmpeg-sys-next, image, rayon |
| `vz-render` | wgpu device, render graph, texture pool, WGSL shaders (blend modes, transforms, masks, chroma, colour, effects, transitions), golden-image tests | wgpu, bytemuck |
| `vz-text` | Fonts (bundled + system + imported), shaping, line breaking, grapheme-safe splitting, text styles → glyph outlines/atlases, caption layout & word timing | cosmic-text, fontdb, swash |
| `vz-vector` | Shapes, bezier paths, gradients, strokes, dashes, trim paths → tessellation or textures | lyon (or vello/tiny-skia), resvg |
| `vz-audio` | Playback engine, mixer, DSP (gain, fades, EQ, compressor, noise reduction, pitch/time-stretch, reverb…), loudness, beat detection, recording | cpal, rubato, ebur128 |
| `vz-export` | Offline render loop, encoders (hardware + software), presets, GIF/still/audio/subtitle export, job queue | ffmpeg-next |
| `vz-presets` | JSON/WGSL data for effects, transitions, filters + generated `.cube` LUTs, animations, text styles, caption styles, stickers, templates | serde |
| `vz-ui` | egui panels: home, top bar, asset panel, player, inspector, timeline, graph editor, dialogs, shortcuts, settings, theme | egui, eframe, egui_dock or own splitters, rfd |
| `vidzcut` | `main()`, window setup, logging, settings, crash recovery, wiring crates together | tracing, directories, anyhow |

**Data-driven presets:** an effect/transition/animation is a small JSON file (name, parameters, which shader) + a `.wgsl` file. Adding the 100th effect = adding files, no core changes.

## 4. Project file (`.vzp`)

- JSON, human-readable, `"format_version": 1`, with `migrate()` functions so old files always open.
- Media referenced by **relative path first, absolute path second**, plus size + quick hash so "Relink" can find a moved file.
- Autosave every 30 s to `…/vidzcut/recovery/`, plus keep the last N manual saves. On start: "vidzcut closed unexpectedly — restore?".
- Everything has a stable UUID, so undo, copy/paste, compound clips, and templates can point at things safely.
- The UI-only state (panel sizes, zoom) lives in a separate settings file, not in the project.

## 5. Quality & testing plan (from spec §13)

- `vz-core`: unit tests for time math, every command's apply/revert, ripple/roll/slip/slide, keyframes, save→load, migrations. These run in a second.
- `vz-render`: render a few known frames headlessly (wgpu works without a window) and compare with stored PNGs (small tolerance). Includes a Sinhala text sample.
- Export test: render 5 s, check duration/size/fps/A-V sync with `ffprobe`.
- Test media is **generated by us** (colour bars, tone, counting numbers) by a small tool, never committed as big files.
- Manual checklists for the owner per phase in `docs/TESTING.md`.
- Each time: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, `cargo build --release`.

## 6. Where I suggest changing or tightening the spec (please read)

### 6.1 egui and Sinhala — **spike result (2026-10-02): PASS for display**
Tested with eframe/egui 0.36.2 and cosmic-text 0.19.0 (`spikes/sinhala`, screenshot `out/sinhala_spike.png`, not committed). With a Sinhala font loaded, **egui draws "ශ්‍රී ලංකාව", "ක්‍රීඩාව", "ඇස් සම්බන්ධ රෝග" correctly (same shapes as cosmic-text)**, in labels and in an editable box. So egui stays (option A) and my worry below was out of date for this version.
Still to check by hand (needs a Sinhala keyboard; the spike has a `--stay` mode): typing, backspace and cursor movement over joined letters in the egui edit box. If cursor/backspace misbehave on clusters, we write the small custom text-edit widget described below.
egui has no Sinhala glyphs by itself, so we must **bundle Noto Sans Sinhala** (OFL) and register it as a fallback font (the spike used Windows' Nirmala UI).

<details><summary>Original concern and options (kept for the record)</summary>

### The big one: egui and Sinhala
`egui` draws its own text with a simple engine that, **as far as I know, does not do "complex script shaping"** (the step that joins Sinhala letters, vowel signs and ්‍ර / ් joiners correctly). English UI text is fine. The problem would be **any place where Sinhala shows up inside the egui interface**: typing Sinhala into the text-clip editor, the caption list editor, project names, font previews.
Video text is *not* affected: we draw video text ourselves with `cosmic-text` (which does shape Sinhala) in `vz-text`.

Options:
- **A (recommended): keep egui; do a 1-day "Sinhala spike" first.** Build a tiny test window that shows "ශ්‍රී ලංකාව", "ක්‍රීඩාව", "ඇස් සම්බන්ධ රෝග" and lets you type. If egui is wrong, we render Sinhala-capable text boxes ourselves: `vz-text` shapes the text with cosmic-text → draws it into an egui texture → we write a small text-edit widget (cursor/selection handled by us on grapheme boundaries). We need a custom widget anyway for in-canvas text editing.
- **B:** switch UI to **iced** (it already uses cosmic-text, so Sinhala works everywhere). Cost: fewer ready-made docking/timeline helpers; more custom UI code.
- **C:** Slint — good shaping, but different licence terms (GPL / royalty-free / commercial). Needs your approval because of the "no GPL without asking" rule.

If the spike shows egui is fine for display and we only need the custom edit widget, we continue with A. I'll report the result with a screenshot before building more UI.
</details>

### 6.2 FFmpeg is not installed on this PC, and neither is Rust
Details in `docs/BUILDING.md`. Short version: I need to install Rust, Microsoft C++ Build Tools, LLVM (for the FFmpeg bindings) and download an **LGPL shared FFmpeg build**. Each is a download — I will ask first.

### 6.3 The software H.264 fallback
An LGPL FFmpeg has **no x264** (that is GPL). So we can export with: NVENC (NVIDIA), QuickSync (Intel), AMF (AMD), VideoToolbox (Mac) — all fine on your two RTX laptops — and for PCs with none of those we need a fallback:
- **A (recommended): OpenH264** (BSD) as the software fallback. Slower and less efficient than x264 but legal to ship; Cisco's licence for the binary needs us to download their prebuilt at install time or build our own (patent fees would be ours then). I'll research and ask before finalising.
- **B:** ship x264 as a **separate** `ffmpeg.exe` program that we run (GPL stays separate from our code, but the installer then carries GPL software; legal advice advised).
- **C:** hardware-only for v1, "software export" later.
Also: H.264/HEVC have **patent pools**. For a commercial release you should ask a lawyer. I am not one.

### 6.4 Hardware *decoding* is a Phase 7 job, not Phase 1
Getting hardware-decoded frames straight into the GPU (D3D11VA → wgpu texture) is tricky. Phase 1 uses fast software decoding + upload; a proxy for 4K covers the rest. Hardware decode comes in Phase 7 ("hardware decoders" is already listed there).

### 6.5 `ffmpeg-next` crate age
`ffmpeg-next` sometimes lags behind the newest FFmpeg. I will check which FFmpeg major version it supports before downloading a build (and fall back to `ffmpeg-sys-next` or a different binding if needed).

### 6.6 egui layout helpers
egui has no docking system built in. Phase 0 will use our own simple splitters (drag borders, save sizes) — a custom layout system is also needed for CapCut-style "Layout" presets. `egui_dock` is optional later.

### 6.7 Process rule
The spec says do not start the next phase before the owner OKs. Inside a phase I will split work into small sub-steps (like 0a, 0b…) and commit each; I will not stop for OK between sub-steps unless I need a decision.

## 7. Phase 0 sub-steps (after you say OK)

| Step | What | You can see |
|---|---|---|
| 0a | Install tools (Rust, C++ build tools, LLVM, FFmpeg LGPL dev files) — each after your OK | `cargo --version` works |
| 0b | Cargo workspace with all crates as empty shells, CI settings, `cargo fmt/clippy/test` green | nothing visible yet |
| 0c | **Sinhala spike** (see 6.1) | a small window with Sinhala text, screenshot to you |
| 0d | Real app window: vidzcut theme (black + yellow), logo, top bar, 10 category tabs, asset panel, player area, inspector, timeline area — all empty but draggable splitters, layout presets, saved sizes | the vidzcut window |
| 0e | Home screen with "Create project" (creates an empty `.vzp` in memory/on disk) and recent list skeleton | home screen → editor |
| 0f | `scripts/setup-windows.ps1` (one-command setup), `docs/BUILDING.md` tested on this PC, macOS instructions + GitHub Actions workflow (written; needs a GitHub repo to run) | one command builds and runs |

**Phase 0 is done when** you can run one command and see the vidzcut window on Windows, and the macOS build instructions exist.
