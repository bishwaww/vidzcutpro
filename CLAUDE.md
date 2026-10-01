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

## Current state (2026-10-01)
- **Phase 0, planning stage.** Documents written, brand assets copied, git initialised. **No Rust code yet.**
- This PC: Windows 11, i5-13420H, RTX 4050 + Intel UHD, 15.6 GB RAM, 125 % display scaling. Installed: git, node, python, winget. **Missing: Rust, MSVC Build Tools, LLVM, FFmpeg.**
- Waiting for the owner's answers to the Phase 0 decisions (see below), then step 0a.

## Open decisions for the owner (Phase 0)
1. OK to install Rust + MSVC Build Tools + LLVM + LGPL FFmpeg build (each a download)?
2. UI toolkit: keep egui after a Sinhala spike (recommended) / iced / Slint? (ARCHITECTURE §6.1)
3. Software H.264 fallback: OpenH264 (recommended) / separate GPL x264 / hardware-only for v1 (ARCHITECTURE §6.3)
4. Gemini auto captions from the prototype: leave out of v1 and add later as a plugin (spec says no AI) — confirm.
5. Project extension `.vzp` (spec) vs `.vidz` (prototype) — plan: `.vzp`, importer for `.vidz` later.
6. Language of my replies: simple English (spec) or Singlish (prototype habit)?

## Known issues
- None yet (no code).

## Next step
Owner answers → step 0a (install tools) → 0b workspace → 0c Sinhala spike → 0d window → 0e home → 0f scripts.
