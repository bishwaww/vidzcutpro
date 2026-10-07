# vidzcut — hand-off for a new chat (written 2026-10-05)

Paste this to the new chat: **"Read docs/HANDOFF.md, then CLAUDE.md and docs/SPEC.md in D:\Projects\vidzcutpro, then continue."**

## 1. What this is
**vidzcut** = a professional desktop video editor in **Rust** (egui/eframe 0.36, CPU compositor with rayon, FFmpeg LGPL, cpal, cosmic-text, tiny-skia). CapCut-PC-style layout + Premiere-style keyframes. No AI / cloud features in v1. Original assets only. Sinhala and Tamil are first-class. Main target: Windows.
- Repo folder: `D:\Projects\vidzcutpro`. Master spec: `docs/SPEC.md` (wins over everything). Working notes: `CLAUDE.md` (architecture + per-phase "additions"), `ROADMAP.md`, `docs/TESTING.md` (owner's hand-test checklists per phase).
- Old Python prototype `D:\Projects\vidzcut` = reference only. Real test video (read-only): `D:/Gloma/with track.mp4`.
- Ignore the CapCut folders (`C:\Users\bishw\AppData\Local\CapCut\Apps`, `F:\All Capcut`). Never read or copy from them.

## 2. The owner and how to work
- The owner is **not a programmer**: plain, simple English, no jargon without a one-line explanation, never ask them to edit code.
- Decisions: give 2–3 options + a recommendation, then wait.
- Work in **phases**; **stop at the end of each phase** with a report: ✅ built / 🧪 please test / ⚠️ known gaps / ➡️ next. Wait for the owner's "OK, continue".
- Each phase also updates ROADMAP.md, CLAUDE.md and the checklist in docs/TESTING.md.
- Standing decisions: tools may be installed; UI stays egui; OpenH264 as software H.264 fallback (**licence still to confirm before release — ask owner**); Gemini captions are out of v1; project extension `.vzp`; Noto fonts and Lucide icons are bundled (approved).
- Ask before: GPL/MPL dependencies (resvg is MPL-2.0 → **not added**, SVG sticker import deferred), paid/cloud features, breaking project-file changes, any download.
- Commit only when `scripts\gate.ps1 -Release` prints **GATE PASSED**.

## 3. Status
| Phase | What | State |
|---|---|---|
| 0–1 | Foundation, media import, preview, export | done |
| 2 | Canvas editing, text, shapes, overlays | done |
| 3 | Keyframes, animations, 35 transitions, speed | done |
| 4 | Colour, 36 filters, 39 effects, masks, chroma key, scopes | done |
| 5 | Pen tool, shapes, advanced text, templates, captions, compound clips, multi-timeline | **built + committed, waiting for the owner's hand test** (`docs/TESTING.md` → "Phase 5 — your checklist") |
| 6 | Pro audio (next) | not started |
| 7 | Export options, settings, installers, user templates | not started |
| 8 | Motion graphics extras | not started |

Last commit: `ff5f2cd Phase 5: pen tool, shape gradients/outlines/trim paths, text templates and groups, compound clips and multiple timelines` (branch `main`). Full gate passed before it.

**Next step:** the owner tests Phase 5 and says OK (or reports bugs) → fix → then **Phase 6**: loudness normalise, noise reduction, EQ, compressor, voice effects, ducking, beat detection + snap to beats, voice-over recording, audio-only export. (Check docs/SPEC.md for the exact audio list.)

## 4. GitHub push — NOT DONE YET
- Remote added: `origin = https://github.com/bishwaww/vidzcutpro.git`; nothing is pushed yet.
- Push is **rejected**: "refusing to allow a Personal Access Token to create or update workflow `.github/workflows/build.yml` without `workflow` scope". Both tokens the owner tried lack that scope.
- Fix options: (1) owner makes a **classic** token with `repo` + `workflow` scopes (or fine-grained with "Workflows: Read and write") and gives it to you; or (2) push without `.github/workflows/build.yml` (rewrites history; only with the owner's OK; the file can be added on the website later).
- Push without saving the token anywhere:
  `git -c credential.helper= -c "http.extraheader=Authorization: Basic $(printf 'x-access-token:TOKEN' | base64 -w0)" push -u origin main`
- A token was pasted in an earlier chat → remind the owner to **revoke it** after the push. **Never write a token into a file or git config.**
- Git is configured with CRLF warnings only (harmless).

## 5. How to build, test, run (Windows)
Bash tool tips: write Python/Rust-heavy edits as script files (Write tool + `python file.py`), because heredocs with apostrophes break; after `cargo fmt` many `Edit` strings stop matching — re-read the file first.
```powershell
$root='D:\Projects\vidzcutpro'
$env:Path="$root\third_party\ffmpeg\bin;$env:USERPROFILE\.cargo\bin;$env:Path"
$env:LIBCLANG_PATH='C:\Program Files\LLVM\bin'
cd $root
powershell -ExecutionPolicy Bypass -File scripts\gate.ps1 -Release   # fmt + clippy -D warnings + all tests + release build
cargo build -p vidzcut        # debug exe: target\debug\vidzcut.exe (fast to build, optimised)
```
- Toolchain: Rust 1.99 (MSVC), VS 2022 Build Tools, LLVM (libclang), FFmpeg n9.0 LGPL shared in `third_party\ffmpeg` (git-ignored).
- **Driving the real window** (screenshots to `target\shots\*.png`): env `VIDZCUT_SCRIPT="wait:20;click:x,y;dclick:x,y;rclick:x,y;drag:x1,y1,x2,y2;key:Alt+G;shot:path.png;quit"`; positions are egui points = screenshot pixels / 1.25 on the owner's PC. Other switches in `crates/vz-ui/src/dev.rs`: `VIDZCUT_START=editor`, `VIDZCUT_CATEGORY=n`, `VIDZCUT_IMPORT=a.mp4`, `VIDZCUT_PLAYHEAD=secs`, `VIDZCUT_NO_AUDIO=1`, `VIDZCUT_CAPTIONS=file.srt;style_id`. **Always set `VIDZCUT_CONFIG_DIR` and `VIDZCUT_LOG_DIR`** to a temp folder so the owner's real settings are untouched.
- egui 0.36 API facts: `App::ui`, `Panel::..show(ui,..)`, `TextEdit::load_state(...).cursor.char_range()` gives `CharIndex` (`.into()` usize), `Popup::from_toggle_button_response`, `Shape::dashed_line` returns `Vec<Shape>` (use `painter.extend`). cosmic-text 0.19, cpal 0.18, ffmpeg-next 9.
- Clippy rules bite: use `as_chunks`, avoid `field_reassign_with_default`, collapse `if let` chains (let-chains are enabled), no `needless_range_loop`.

## 6. Architecture cheat-sheet (details in CLAUDE.md)
- Crates: `vz-core` (project model, commands + undo, Document), `vz-media` (probe, decode), `vz-render` (CPU compositor, used by player AND export), `vz-audio` (Mixer, cpal engine), `vz-export`, `vz-anim`, `vz-text`, `vz-vector`, `vz-presets` (animations, transitions, effects, filters, captions styles, text templates), `vz-ui` (egui app), `vidzcut` (main).
- Time is `i64` microseconds. All edits go through `Command`s (`Document` methods) with undo/redo and merge keys; the UI never edits the project directly.
- **History entries remember which timeline they were made in; undo/redo switch back to it.** `Project.active` says which timeline `timeline()` returns. Export exports the open timeline.
- Render order per layer: chroma key → colour → effects → masks → animation blur → composite. Compound clips are rendered twice (on black and on white) and un-matted to get transparency; never pasted (`is_plain`); depth limit 6.
- Sizes of text/shapes are in *reference pixels* (short canvas side = 1080); `Transform.x/y` are canvas pixels.

## 7. Known gaps (also in ROADMAP.md)
- Owner has **not yet tested**: Sinhala typing in egui edit boxes; real listening to audio; sound/picture offset from device latency (not compensated); Phase 5 whole checklist.
- Not built: SVG sticker import (needs OK for MPL resvg), mask drawn with the pen on the picture, keyframes for shape/path/look/effect values, "break apart" compound clip, sound of a retimed compound clip, user presets / user templates (Phase 7), graph editor, GPU compositor, macOS build untested, GitHub Actions never ran.
- Audio tab and Templates tab show "Coming in Phase N".
- Compound clips cost 2× render time (watch playback speed on 1080p).

## 8. Where Phase 5 code lives (quick map)
Pen tool `crates/vz-ui/src/editor/pen.rs` · shape inspector `inspector/shape.rs` · shape drawing `crates/vz-vector/src/lib.rs` · templates `crates/vz-presets/src/templates.rs` + `editor/template_panel.rs` · groups `vz-core/src/document/group.rs` · timelines + compound `vz-core/src/document/timelines.rs` · timeline tabs `editor/timeline.rs::tabs` · renderer nesting `vz-render/src/lib.rs` (`render_timeline`, `nested`, `unmatte`) · mixer recursion `vz-audio/src/mixer.rs` (`mix`) · captions `vz-core/src/captions.rs`, `document/caption_ops.rs`, `editor/caption_panel.rs` · rich text `crates/vz-text`.
