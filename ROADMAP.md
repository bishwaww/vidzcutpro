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

## Phase 2 — Multi-track compositing & text
- [ ] Overlay tracks, images, canvas ratios + backgrounds, opacity, blend modes, crop
- [ ] On-canvas handles (move/scale/rotate), smart guides, safe zones
- [ ] Audio tracks, waveforms, volume, fades
- [ ] Snapping, magnet, linkage, multi-select, copy/paste, markers
- [ ] Text clips + styling + **Sinhala/Tamil shaping**, bundled fonts
- [ ] Stickers (import + a few built-in)

**Done when:** the owner can make a 9:16 short with a talking-head video, a logo overlay, a Sinhala headline and background music, and export it.

## Phase 3 — Animation
- [ ] Keyframes on every property, easing presets + custom bezier, prev/next keyframe
- [ ] In / Out / Combo animation presets (data files)
- [ ] Transitions library (first 15+)
- [ ] Speed: standard, curve presets + editor, keep pitch, reverse, freeze frame

**Done when:** a title can fly in with ease-out, a clip can zoom slowly with keyframes, and cuts can use transitions.

## Phase 4 — Color & effects
- [ ] Adjust: basic, HSL, curves, colour wheels, LUT (.cube import)
- [ ] Filters library (own LUTs), adjustment layers
- [ ] Effects library (first 25+), masks (all shapes + pen), chroma key, scopes

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
