# vidzcut roadmap

Source: `docs/SPEC.md` §12. Tick items when they are built **and** checked. A phase is finished only when its "Done when" line is true and the owner has tested it.

Legend: `[x]` done · `[ ]` to do · `[-]` skipped on purpose (reason given)

## Phase 0 — Study & foundation  ✔ done
- [x] Read the old prototype → `docs/PROTOTYPE_NOTES.md`
- [x] Copy brand assets → `assets/brand/`
- [x] Plan → `docs/ARCHITECTURE.md`, `docs/BUILDING.md`, this file, `CLAUDE.md`
- [x] Owner OK on the plan + decisions (2026-10-02, see `docs/SPEC.md` "Spec changes")
- [x] 0a Install tools: Rust 1.99, C++ Build Tools, LLVM 23, LGPL FFmpeg n9.0; FFmpeg verified from Rust (`vz-media` test)
- [x] 0b Cargo workspace with all crates; fmt + clippy + test green
- [x] 0c Sinhala spike: **egui 0.36 shows Sinhala correctly** (typing/cursor still to be checked by hand)
- [x] 0d App window: theme, logo, top bar, 10 category tabs, asset panel, player, inspector, timeline (empty, ruler), draggable splitters, 3 layout presets + reset, sizes remembered
- [x] 0e Home screen with "Create project" (project kept in memory; saving is Phase 1)
- [x] 0f `scripts/setup-windows.ps1`, `run-vidzcut.bat`, BUILDING.md, macOS instructions, GitHub Actions file (**not yet run: needs a GitHub repo; macOS untested**)
- [x] First real `vz-core` pieces: integer-µs time + exact frame rates, versioned `.vzp` project settings with load/save tests
- [x] Owner approved Phase 0 and said "continue" (Phase 1 started 2026-10-02). The Sinhala *typing* check (cursor/Backspace over joined letters) is still open; it is part of the Phase 1 hand test.
- Known small gaps carried forward: tab icons are placeholder letters (Lucide icons planned); Noto Sans Sinhala/Tamil not bundled yet (Windows' Nirmala UI is used temporarily); GitHub repo for macOS CI.

**Done when:** the owner runs one command and sees the vidzcut window on Windows, and the macOS build instructions exist.

## Phase 1 — Minimum real editor  ← built, waiting for the owner's hand test
- [x] 1a Project model, commands + undo/redo (merged drags), save/open `.vzp`, autosave every 30 s + crash recovery, relink of moved media (53 tests)
- [x] 1b Import media (button, drag-drop from Explorer, many files), probing in a background thread, thumbnails, media grid ("Added" / "Missing" badges, + button, double-click, right-click)
- [x] 1c Frame-accurate decoder (phone rotation, HD colour matrix BT.709), shared renderer (player and export use the same code), live preview thread, scrubbing
- [x] 1d Sound: reader + mixer + sound-device engine; the sound card's clock drives playback; verified on the real device
- [x] 1e Main-track editing: drag in from the media panel, reorder by dragging, trim by dragging edges, split (Ctrl+B), delete + ripple, Q / W, undo/redo, markers of selection, zoom, scroll, Fit
- [x] 1f Save / Save as / Open, recent projects on the home screen, quit prompt, recovery prompt after a crash
- [x] 1g Export 1080p (also 480p–4K) H.264 + AAC MP4: tries NVENC → QuickSync → AMD → OpenH264; progress, cancel, "Open folder"
- [x] The project adopts the first clip's shape and frame rate (one undo step)
- Verified with real footage (`D:/Gloma/with track.mp4`, 1080p 24 fps): plays at a steady 30 pictures/s; a 10 s 1080p export took 10.8 s (28 fps, Intel QuickSync).
- Known limits carried forward: export speed is CPU-bound (Phase 7 improves it: threads, GPU colour conversion, hardware decode); NVENC needs NVIDIA driver 610+ with this FFmpeg build (this PC has 592, so QuickSync is used); no waveforms/filmstrips beyond one thumbnail per clip (Phase 2); only the main track exists (Phase 2 adds overlays/audio tracks); a project's frame-rate change after editing does not re-snap clips.

**Done when:** the owner can cut a 2-minute video into pieces, reorder them, save, reopen, and export a playable MP4 identical to the preview.

## Phase 2 — Multi-track compositing & text  (built 2026-10-03, waiting for the owner's hand test)
- [x] Overlay tracks, images, canvas ratios + backgrounds (colour / blur), opacity, 18 blend modes, crop
- [x] On-canvas handles (move/scale/rotate), snap guides to the canvas edges and middle, thirds + safe-zone overlays (state exists; no button yet), arrow-key nudging (Alt+arrows)
- [x] Audio tracks, waveforms, volume (also animatable in the model), fades, mute
- [x] Snapping (clip edges, playhead, markers), magnet, multi-select, copy / cut / paste / duplicate (Ctrl+C/X/V/D), markers (M)
- [x] Text clips + styling (font, size, B/I/U, colour, align, spacing, outline, shadow/glow, background box, wrap) + **Sinhala/Tamil shaping**, bundled fonts, 12 text styles
- [x] Stickers: 12 built-in stickers and 12 plain shapes (own artwork, vector); PNG/GIF/WebP/APNG import through Media
- [ ] Not yet: typing directly on the canvas (double-click opens the text box in the inspector instead), SVG sticker import (needs the owner's OK for resvg, MPL-2.0), "linkage" between video and its sound (field exists, no behaviour), safe-zone / grid buttons, GPU compositor (CPU renderer on all cores is fast enough for now)

**Done when:** the owner can make a 9:16 short with a talking-head video, a logo overlay, a Sinhala headline and background music, and export it.

## Phase 3 — Animation  (built 2026-10-03, waiting for the owner's hand test)
- [x] Keyframes on position, size, rotation, opacity and volume: diamond buttons, previous/next keyframe, auto-keyframe while dragging on the picture, keyframe diamonds on the selected clip, right-click a diamond for "how it moves to the next keyframe" (linear, hold, ease in/out, and sine / quad / cubic / back / elastic / bounce curves) and "stop animating". A custom curve (cubic bezier) exists in the engine.
- [x] In / Out / Combo animations as data (15 In, 15 Out, 6 Combo: fade, zoom, slide, spin, swing, pop, bounce, blur, wipe, shake, turn, wobble, pulse, float...), length slider; a combo replaces In and Out
- [x] Transitions: 35 (mix, fades, blur mix, slides, pushes, zoom, spin, wipes incl. clock / iris / diamond / barn door / blinds / split, flash, pixelate, glitch, flip, cube); live preview on hover, badge on the cut, length + easing, "apply to all cuts"; centred on the cut, first/last picture held when a clip has no more picture
- [x] Speed: 0.1x to 100x (slider, quick buttons, or type the length), speed curves (6 presets + draggable points), reverse, freeze frame (inspector button and toolbar), "keep pitch" sound (own WSOLA time stretching) or tape-style pitch change; trimming and cutting follow the speed; reverse playback of video is smooth (pictures decoded in runs)
- [ ] Not yet: graph editor panel, custom bezier handles in the UI, motion path on the canvas, copy / paste keyframes, dragging keyframes on the timeline, keyframes for crop / text / shape / colour properties, text animators (per letter), sound crossfade at transitions (the picture blends, the sound does not overlap), smooth slow motion (frame blending), hover preview for In/Out/Combo animations, "apply animation to all"

**Done when:** a title can fly in with ease-out, a clip can zoom slowly with keyframes, and cuts can use transitions.

## Phase 4 — Color & effects  (built 2026-10-03, waiting for the owner's hand test)
- [x] Adjust: basic (17 sliders: temperature, tint, saturation, vibrance, exposure, brightness, contrast, highlights, shadows, whites, blacks, brilliance, sharpen, clarity, fade, vignette, grain), HSL (8 colour bands), curves (RGB + R/G/B, draggable points), colour wheels (lift / gamma / gain / offset), LUT (`.cube` import, strength) plus 4 looks of our own; "Apply to all clips"
- [x] Filters library: 36 filters in 9 groups (each is a set of adjustments, fine-tunable on the Adjust tab)
- [x] Adjustment layers (own effect track, with masks and opacity)
- [x] Effects library: 39 effects (blur, zoom/spin/motion blur, sharpen, vignette, grain, letterbox, mirror, flip, split screen, glow, flash, strobe, chromatic aberration, prism, light leak, lens flare, VHS, old film, B&W, sepia, CRT, pixels, halftone, camera shake, wave, swirl, fisheye, bulge, ripple, glitch, RGB split, digital noise, kaleidoscope, tile, pulse zoom, flicker, colour cycle); on one clip (stackable, reorderable) or as an effect clip over everything below
- [x] Masks: linear, mirror, circle, rectangle (rounded), heart, star; position, size, rotation, feather, invert; several per clip; also on adjustment layers
- [x] Chroma key: key colour, intensity, shadow, edge feather, spill removal
- [x] Scopes window: waveform, vectorscope, histogram
- [ ] Not yet: masks drawn / dragged directly on the picture and the pen mask (the pen tool arrives in Phase 5), keyframes for colour / effect / mask values, saving your own presets, "copy and paste attributes", picture-frame effects, stabilize, tracking, GPU compositor (everything still runs on the CPU, all cores: 1080p effects take 3-120 ms each; the GPU comes if Phase 7 measurements need it)

## Phase 5 — Vector, advanced text & captions
- [ ] Shapes + pen tool, gradients, colour picker with eyedropper, stroke, trim paths
- [ ] Advanced text: multi-stroke, shadow, glow, curved, per-word styling, text animators, templates
- [ ] Captions: manual, import/export SRT/VTT/LRC/ASS/TXT, list editor, prototype's caption styles, apply-to-all
- [ ] Compound clips + multiple timeline tabs

## Phase 6 — Pro audio
- [ ] Loudness normalize, noise reduction, EQ, compressor, voice effects, ducking
- [ ] Beat detection + snap to beats, voiceover recording, audio-only export

## Phase 7 — Export & polish
- [ ] All export options (§11.1), hardware encoders, proxies, hardware decode, render cache
- [ ] Settings, shortcut editor, templates (save/use), home polish, error dialogs
- [ ] Installers (Windows + macOS), first-run sample project, `THIRD_PARTY_LICENSES.md` complete

## Phase 8 — Motion graphics extras
- [ ] Graph editor, motion paths on canvas, parenting/null layers
- [ ] Motion blur, wiggle, animation-preset saving
- [ ] Motion tracking + stabilisation (non-AI)

## Out of scope in this version (spec §0)
- [-] Auto captions / speech-to-text (plugin hook only)
- [-] AI background removal, cutout, retouch, avatars, text-to-speech, AI colour match, smart suggestions, clipper, auto reframe, AI effects, copyright check
