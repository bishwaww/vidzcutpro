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

## Current state (2026-10-02) — Phase 0 built, waiting for the owner's hand test
- Workspace builds clean: `cargo fmt`, `clippy -D warnings`, `test --workspace` (~30 tests), `build --release`.
- Real code so far: `vz-core` (`time.rs` integer-µs time + exact `FrameRate`; `project.rs` versioned `.vzp` settings, JSON load/save + migration hook), `vz-media` (FFmpeg init + version, DLL-copy `build.rs`), `vz-ui` (theme, settings, home screen, editor shell: topbar / assets / player / inspector / timeline, all empty but resizable), `vidzcut` (main + file logging). `spikes/sinhala` = throwaway Sinhala test. Other crates are empty shells.
- UI facts: egui/eframe **0.36** (`App::ui`, `Panel::left/right/top/bottom(..).show(ui, ..)`, `ctx.global_style_mut`). cosmic-text **0.19** (`buffer.borrow_with(&mut fs)` pattern). Panel sizes saved in `%APPDATA%\vidzcut\config\ui_settings.json`; logs in `%LOCALAPPDATA%\vidzcut\data\logs`.
- Dev switches for automatic screenshots: `VIDZCUT_SHOT`, `VIDZCUT_START=editor`, `VIDZCUT_LAYOUT`, `VIDZCUT_CATEGORY`, `VIDZCUT_CONFIG_DIR`, `VIDZCUT_LOG_DIR` (see docs/BUILDING.md). **Always set the two DIR variables when checking so real settings are untouched.**
- Windows build env: `LIBCLANG_PATH=C:\Program Files\LLVM\bin`, `FFMPEG_DIR` from `.cargo/config.toml`. In a plain tool shell add `%USERPROFILE%\.cargo\bin` to PATH.
- This PC: Windows 11, i5-13420H, RTX 4050 + Intel UHD, 15.6 GB RAM, 125 % display scaling.
- Tool tips: the Bash tool breaks on long heredocs containing quotes → use the Write tool for files. egui's own panels remember sizes by id; we change the id suffix (`layout_gen`) to force a reset.
- Not done / unverified: typing + Backspace + cursor over joined Sinhala letters in egui's text box (owner to test; fallback = custom cosmic-text edit widget); macOS build; GitHub Actions (no repo yet); setup-script "install" branches (only the "already installed" path was run); tab icons are letters.

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
- Asset tab icons are placeholder letters. Sinhala fonts: only Windows' Nirmala UI via system path (macOS/Linux paths guessed, untested). Export / Save / Import are disabled ("Coming soon").

## Next step
Owner runs the Phase 0 checklist in `docs/TESTING.md` and says OK (or reports problems) → **Phase 1** (ROADMAP.md): project commands + undo, save/open + autosave, import + media grid, decode + playback with audio clock, main-track editing, 1080p H.264 export. Tell the owner the Phase 1 sub-steps (1a, 1b, …) at the start.
Before Phase 2: ask the owner to approve downloading the Noto Sans Sinhala / Tamil + Inter fonts (OFL) and Lucide icons (ISC).
