# Notes on the old vidzcut prototype

Read on 2026-10-01 from `D:\Projects\vidzcut` (read-only; nothing in that folder was changed).
Sources: `README.md`, `AGENTS.md`, `PROGRESS.md`, `SPEC.md` (not read in full), the very long `CLAUDE.md` (hand-off log, ~210 KB), the module list with file sizes, and the Sinhala/caption code in `vidzcut/text.py`.
I did **not** run the prototype or its 693 tests. Everything below is from its own documentation and code, so treat "works" as "the prototype's notes say it works".

## 1. What it is

| | |
|---|---|
| Version | 1.4.0 (installer `dist\vidzcut-Setup-1.4.0.exe`, ~115 MB) |
| Language / UI | Python 3.12 + **PySide6 (Qt 6)** |
| Video | PyAV (FFmpeg bindings), OpenCV, NumPy; compositing is **CPU** (Qt `QPainter` + NumPy/OpenCV), not GPU |
| Export | frames piped into an FFmpeg process; GPU encoders (NVENC/QSV/AMF) auto-picked, libx264 fallback. Measured 100–250+ fps for simple 1080p |
| Audio | whole timeline mixed to one WAV by FFmpeg filter graphs, played by Qt Multimedia |
| Installer | PyInstaller folder (~406 MB) + Inno Setup wizard with custom dark artwork |
| Size | ~60 Python modules, ~690 automated tests |
| Project file | `.vidz` = JSON, integer **microsecond** times, old files keep opening (unknown keys ignored) |

It is a real, fairly complete editor (its log shows work from 2026-09-25 to 2026-10-01). The new Rust project is a rewrite, not a port.

## 2. Features it already has (the bar the new build must reach)

- **Home screen:** big "Create project", shape chips (16:9, 9:16, 1:1, 4:3, 3:4, 4:5, 21:9), recent/drafts grid with covers, right-click Rename/Duplicate/Delete/Show in folder, Continue-last-session, crash recovery, splash screen.
- **Editor shell:** top bar (Menu, auto-save time, project name, Export), top category bar (Media, Audio, Text, Stickers, Effects, Transitions, Captions, Filters, Adjustment), left asset panel with sub-categories + search + favourites + hover preview, player, Properties panel, timeline.
- **Layouts:** Default, Vertical (player full height on the right), Media, Generative, Attributes, Reset. Sizes remembered per layout.
- **Timeline:** multi-track (video, text, adjust, audio), track lock/hide/mute, resizable track rows, magnet, snapping, linkage, markers, groups, loop range (I/O), preview axis, Cover box, auto-scroll while dragging, blade tool, nudge by frame, copy/paste/duplicate, delete-left/right (Q/W), copy/paste attributes, presets, 500-step undo (whole-project copies), **multiple timelines** as tabs, **compound clips** editable in place.
- **Properties:** Video (Basic: transform with ◇ keyframes, blend, crop, canvas background, effect; Remove BG = chroma key; Mask), Audio (volume dB, fades, normalize, voice changer, pitch, 3-band EQ, noise reduction, ducking, beats), Speed (Normal + Curve, reverse, freeze), Animation (In/Out/Combo), Adjust (11 sliders, 15 filters, LUT), section-level keyframe diamond with 4 states.
- **Keyframes:** position/scale/rotation/opacity/volume/colour/mask/chroma, 15 easings + cubic-bezier handles, a graph editor (Alt+K), multi-select.
- **Text:** 22 looks, stroke/shadow/glow/gradient/background, word modes (karaoke, reveal, single word, keyword), 30+ in/out/loop animations, caption list editor, SRT import/export.
- **Shapes & stickers:** ~40 drawn vector shapes/stickers/badges, emoji from the Windows emoji font, custom PNG/GIF import, a pen tool with bezier handles.
- **Effects:** 12 pixel effects (shake, zoom pulse, RGB glitch, VHS, old film, blur, flash, pixelate, mirror, vignette pulse, light leak, B&W flicker). **Transitions:** 17. **Filters:** 15 + `.cube/.3dl` LUTs.
- **Audio extras:** voiceover recording, beat detection, ducking, 13 voice effects, cross-fades at transitions, limiter.
- **Export:** MP4/MOV (H.264/HEVC), GIF, still frame, audio only (MP3/WAV/M4A), part export, 720p–4K, estimate of file size, progress, cancel.
- **Misc:** proxies for long videos (preview only), missing-file relink window, Windows "Edit with vidzcut" right-click entry, settings (cache, hardware, default fps…), keyboard shortcut list.
- **Auto captions** via Google Gemini (Sinhala + 23 languages) — **cloud AI**, see §6.

## 3. UI layout it settled on (after 4 rounds of owner feedback)

Matches section 5 of the spec almost exactly: top bar → category bar → left asset panel / player / properties → timeline with tabs and a toolbar. The owner's repeated complaints, which the Rust UI should avoid from day one:

1. Track headers must show **icon + name on top, eye / speaker / lock below** (see `pannel head vidzcut.png` in the old folder — a screenshot of the owner's desired header).
2. Properties panel: **no empty scrolling area** on short tabs; one consistent left edge; section headers `Title … ↺ ◀ ◇ ▶` must never overlap.
3. Player selection must be reliable: top-most layer wins, handles easy to grab, click empty canvas deselects, Alt+click cycles overlapping layers, snap guides.
4. Layout "Vertical" = player full height on the right, timeline under the left column.
5. Pen tool must work like the owner's CapCut recording: click = point, drag = bezier handle, click first point = close, double-click = finish open path, Esc = cancel, edit path later.
6. Installer text/buttons must be visible on dark backgrounds (they had black-on-dark bugs).
7. Nothing may be a "fake" switch. If it is not built, hide it or mark "Coming soon".

## 4. How it renders text — especially Sinhala

- Text is turned into `QPainterPath` outlines at a fixed reference size (100 px) using **Qt's text engine, which shapes Sinhala correctly** (checked: ශ්‍රී, vowel signs). Result is cached as a sprite per output size (4× faster export).
- Font fallback for Sinhala = Windows **Nirmala UI / Iskoola Pota** (system fonts, not bundled). The new project should **bundle** Noto Sans/Serif Sinhala (OFL) so output is identical on every PC and on macOS.
- Letter height follows CapCut's formula: `size × short_side / 150`.
- Styling: fill, gradient, outline, shadow, glow, background box, bold/italic, spacing, case. Words are laid out as tokens `(word, role, line)`; roles normal/spoken/active get their own colour; when the active word is bigger, every word gets a slot as wide as its big version so the line never jiggles. **Nice trick worth keeping.**
- **Real test hazard:** typing Sinhala through `QTest.keyClicks` crashed their test process; they used `insertPlainText`.
- Old FM-style Sinhala fonts (non-Unicode) were only used by a *separate* project (SinhalaSubs) to write into CapCut drafts. **Not needed here.**

## 5. Captions

- A caption track is a text track flagged `captions=True`. One caption = one text clip with words in it.
- **No word timings are stored**: Gemini returns none, so karaoke/reveal modes **share each caption's time across its words in proportion to word length**. Same fallback is allowed by the new spec (§6.7). Imported SRT also has no word times.
- Style looks (the owner's favourites, to re-create as data presets):

  | Look | Mode | Key settings |
  |---|---|---|
  | Karaoke Yellow | karaoke | highlight + spoken `#FFD60A`, outline 0.10 |
  | Green Pop | karaoke, 4 words/caption | highlight `#3CFF6B`, scale 1.15, bold, outline 0.12 |
  | Reels 3 Words | karaoke, 3 words, UPPERCASE | highlight `#00E5FF`, bold, outline 0.12 |
  | One Word Pop | single word | white, scale 1.2, outline 0.14, pop-up intro 0.15 s |
  | Word Reveal | reveal | highlight `#FFD60A`, outline |
  | Red Keyword | keyword | highlight `#FF4040`, scale 1.2, bold, outline |

  Plus Classic White, Soft Shadow, Sunny Yellow, Bold Outline, Gold Fade, Black/Yellow/White Box, Neon Blue/Pink, Warm Glow, Fade In & Out, Slide Up, Pop Up, Typewriter.
- "Change all captions together" is on by default (= spec's "Apply to all captions").

## 5b. Compound clips, speed, animation (how they did it, and what was fragile)

- **Compound clip:** the selection is **rendered to a video file** (`<project> compounds/compound ….mp4`) and replaces the clips; the original tracks are stored inside the clip so it can be "broken apart" or edited in place (re-rendered on exit). Works, but it is a **baked file**, so quality is capped and edits cost a re-render. **The Rust design (spec §4.2: `Compound(timeline_id)`) is better — evaluate the nested timeline live.**
- **Multiple timelines:** only the *active* timeline lives in the Project fields; the others are parked as JSON dicts. A scoped-down hack to avoid refactoring; **the Rust model should have `Vec<Timeline>` from the start.**
- **Speed:** one function `source_time(clip, t)` maps timeline time → file time (speed, reverse, 8 curve presets with the same average speed, freeze). Audio: `atempo` chain / `asetrate`, curves via 24 pieces. Good idea: **one place does the time mapping** and everything (video, audio, thumbnails, export) calls it.
- **Keyframes:** `at(clip, t)` returns "the clip as it looks at t"; renderer, player handles, inspector all use it. Same pattern should be kept (`evaluate`).
- **Animations (in/out/combo):** pure maths returning `(alpha, scale, dx, dy, rotation, blur)`; 12 in / 11 out / 9 combo for pictures; text: 12 in / 11 out / 7 loop. Combo replaces in/out. On split: left keeps intro, right keeps outro.
- **Transitions:** live on the *first* clip, centred on the cut; no timeline movement; clip A shows frames past its end, clip B frames before its start. Needs the clips to touch. Same idea is right for the spec ("transition sits between two adjacent clips").

## 6. What looked fragile / lessons

- **Whole-project-copy undo** (fine for a prototype; the spec's command-based undo is better).
- **CPU compositing** limits multi-layer 4K; expected to be far better on wgpu.
- Qt Multimedia quirks: `setPosition` ignored while loading, loop jump pauses ~0.4 s (sound restarts like a seek), unreliable first frames. The new plan (cpal + audio-clock master) avoids these.
- Hard-won playback design worth copying: **audio clock is master**; hold the first frame until sound runs; small drift is slewed, big drift (>300 ms) re-syncs; seek while playing restarts playback.
- A **flaky test hang** from half-destroyed Qt Multimedia threads. (Not relevant to Rust, but shows: shut down audio/decoder threads cleanly.)
- **Windows DPI bug:** blank panels at 125 % scaling because the app was not DPI-aware. winit/egui handle this, but **test at 125 % and 150 %** — the owner's laptop is 125 %.
- Screenshot checks lesson: offscreen screenshots do not show DPI bugs; do at least one real on-screen check.
- Several features were "built then never shown" (a hint label created and `deleteLater()`'d) — **look at every screen with your eyes**.
- Test video used by the prototype: `D:/Gloma/with track.mp4` (1080p24 H.264, 28 min, 1.1 GB). Good real-world test file (read-only).
- Never verified by the prototype: voiceover with a real microphone, voice effects by ear, chroma key on real footage, real hardware 4K export on other GPUs.
- Owner's test laptops: Dev PC (this one: i5-13420H, RTX 4050 + Intel UHD, 15.6 GB RAM, 125 % scaling) and a second laptop (RTX 4050 6 GB, Win 11).

## 7. Ideas / algorithms worth keeping (re-implement in Rust, do not copy code)

| Idea | Why it is good |
|---|---|
| Time is integer µs; frame grid via `round(n·1e6/fps)`, snap everything to frames | Exactly what spec §4.2 wants; avoids drift |
| `source_time()` + `at()` as single sources of truth | One place to fix bugs |
| Beat detection: spectral-flux onsets (11 kHz, hop 256), tempo by autocorrelation 60–180 BPM with a ~120 prior, Ellis-style DP chain, "bass or between" correction. Test loop 120 BPM → 40/40 beats ±21 ms | Non-AI and proven |
| Ducking: RMS per 50 ms, speech threshold −40 dBFS, bridge pauses < 0.6 s, drop sounds < 0.15 s, 0.2 s down / 0.5 s up slopes. **Known weakness:** threshold is absolute, so music under speech keeps ducking all the time — use a *relative* threshold | Good start, known fix |
| Loudness: EBU R128 measured **as stereo** (mono measured as mono is 3 dB off), target −14 LUFS, limiter 0.98 after the mix (+19 dB boost clipped without it) | Real bugs found |
| Voice effects recipes (deep, chipmunk, robot, echo, hall, telephone, radio, megaphone, underwater, choir, wobble…), each ends with loudness compensation (±0.4 LU). **Never use FFmpeg's `vibrato`** (random noise/silence in 7.1) | Saves research time; Rust needs its own DSP |
| Chroma key: Cr/Cb distance measured along/sideways to the key colour direction, shadow counts half, despill removes key colour at same brightness, key at half size above 720 p | Good, cheap algorithm; becomes a shader |
| Crop/mask math that keeps the picture still while the crop window moves | Nice UX |
| Text sprite caching; every word has a fixed slot for karaoke | Performance + polish |
| Fast-export path: single plain clip → straight transcode, with verification | Big speed-up for simple exports |
| Missing-media relink: same kind, length within 1 %/0.2 s, search a folder (200 000-file limit) | Sensible rules |
| Autosave to a recovery file + "keep unsaved work before New/Open" | Prevented data loss |
| Mixed-quality preview: `Full/High/Medium/Low` = max picture side 960/720/480/320 | Simple, effective |

## 8. Things in the prototype that **conflict with the new spec** (owner decision needed)

1. **Gemini auto captions** = cloud AI. Spec §0 says "No AI / cloud in this version". The prototype notes call Sinhala auto-captions the *flagship, commercial selling point*. → Plan: leave out of v1 as the spec says, but design the `FeatureProvider` hook so the Gemini pipeline can be added as a plugin soon. **Please confirm.**
2. **File extension:** prototype `.vidz`, spec `.vzp`. → Plan: new format `.vzp`; later add a small importer for `.vidz`.
3. **Python prototype's FFmpeg** (imageio-ffmpeg 7.1 build) is not guaranteed LGPL. Not reused.
4. Prototype GUI uses Windows' system emoji font for stickers. Spec wants our own/Twemoji (CC-BY) / Noto Emoji (OFL) sets. → bundle an open emoji set.
5. Prototype notes were written to reply to the owner in **Singlish**. The spec says simple English. → I will use simple English unless you tell me to switch.

## 9. Brand assets copied

`assets/brand/`: `logo_source.webp` (original logo), `logo.png` (transparent logo for the home screen), `mark.png` (yellow play mark for the top bar), `icon.png`, `icon.ico` (app icon).
Brand colours from `ui/theme.py`: accent `#FCC610`, accent hover `#FFD84D`, accent dark `#2E2606`, text-on-accent `#0A0A0A`, background `#0A0A0A`, panel `#151515`, panel 2 `#202020`.

## 10. What I deliberately did not do

- Did **not** open, read or copy anything from `C:\Users\bishw\AppData\Local\CapCut\Apps` or `F:\All Capcut`. (Attaching them to the chat made the tool show their top-level file/folder names once; nothing inside was opened.) They are CapCut's own program/data (spec §0: ignore them). **They are not needed for this project.** (`F:\All Capcut` holds your own CapCut drafts; vidzcut can't and shouldn't read those.)
- Did not copy any prototype source code.
