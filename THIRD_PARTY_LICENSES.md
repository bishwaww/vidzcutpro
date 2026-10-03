# Third-party licences

Every library, font and asset that ships with vidzcut is listed here with its licence.
Rule (spec §0): MIT / Apache-2.0 / BSD / ISC / OFL (fonts) / CC0 only. FFmpeg is LGPL and dynamically linked. No GPL without the owner's approval.

Status (2026-10-03, end of Phase 2): **direct dependencies only**. A full check of every indirect dependency (e.g. with `cargo-deny`) is planned for Phase 7, before any release.

## Rust crates (direct dependencies)
| Crate | Version | Licence | Used in |
|---|---|---|---|
| eframe, egui | 0.36 | MIT OR Apache-2.0 | vz-ui (window + interface) |
| cosmic-text | 0.19 | MIT OR Apache-2.0 | vz-text (shaping Sinhala/Tamil/Latin, laying out text) |
| tiny-skia | 0.11 | BSD-3-Clause | vz-vector (drawing shapes and stickers) |
| rayon | 1 | MIT OR Apache-2.0 | vz-render (using all processor cores) |
| image | 0.25 | MIT OR Apache-2.0 | logo/icon decoding, screenshots |
| serde, serde_json | 1 | MIT OR Apache-2.0 | project + settings files |
| uuid | 1 | Apache-2.0 OR MIT | vz-core ids |
| thiserror | 2 | MIT OR Apache-2.0 | vz-core errors |
| directories | 6 | MIT OR Apache-2.0 | config/log folders |
| tracing, tracing-subscriber | 0.1 / 0.3 | MIT | logging |
| cpal | 0.18 | Apache-2.0 | vz-audio (sound device) |
| rfd | 0.17 | MIT | vz-ui (open/save dialogs) |
| chrono | 0.4 | MIT OR Apache-2.0 | vz-ui (local time shown in the top bar) |
| crossbeam-channel | 0.5 | MIT OR Apache-2.0 | vz-media, vz-audio, vz-ui (messages between threads) |
| ffmpeg-next, ffmpeg-sys-next | 9.0 | **WTFPL** (very permissive "do what you want"; not on the spec's list, flagged for the owner) | vz-media (FFmpeg bindings) |

## Native libraries
| Library | Licence | How it is used |
|---|---|---|
| FFmpeg n9.0 (pre-built by BtbN, "lgpl-shared" build) | **LGPL-3.0** (built with `--enable-version3`, no `--enable-gpl`, x264/x265 disabled) | decode/encode; dynamically linked (DLLs next to vidzcut.exe). We must ship the licence text, say that vidzcut uses FFmpeg, and offer the FFmpeg source. Not committed to git (`third_party/`), downloaded by `scripts/setup-windows.ps1`. |
| OpenH264 (inside that FFmpeg build as `libopenh264`) | BSD-2-Clause; Cisco's patent licence applies to Cisco's own binary | planned software H.264 fallback. **Licence and patent terms to be confirmed with the owner before release.** |

## Fonts (bundled in `assets/fonts/`, downloaded 2026-10-03 from the Google Fonts repository)
| Font | Licence | Used for |
|---|---|---|
| Inter (variable) | SIL OFL 1.1 (`Inter-OFL.txt`) | default video text font |
| Noto Sans Sinhala (variable) | SIL OFL 1.1 (`NotoSansSinhala-OFL.txt`) | Sinhala in video text and in the interface |
| Noto Serif Sinhala (variable) | SIL OFL 1.1 (same Noto Project licence) | Sinhala serif text style |
| Noto Sans Tamil (variable) | SIL OFL 1.1 (same Noto Project licence) | Tamil in video text and in the interface |
| egui's built-in fonts (Ubuntu-Light, Hack, emoji icon fonts, Noto Emoji) | OFL / Ubuntu Font Licence / MIT (as shipped inside egui) | interface (Latin) |
| Windows' Nirmala UI / other system fonts | Microsoft / system fonts, only read from the user's own computer, never copied or shipped | last-resort fallback for symbols in the interface; also offered in the font list for video text |

## Icons / artwork
| Asset | Licence | Notes |
|---|---|---|
| vidzcut logo, mark, app icon (`assets/brand/`) | Owner's own artwork | copied from the prototype |
| Lucide icon font (`assets/icons/lucide.ttf`, licence in `LICENSE-lucide.txt`), downloaded 2026-10-03 | ISC | tab icons (more icons later) |

## Not allowed / never used
- Anything from CapCut (code, binaries, icons, effects, LUTs, stickers, templates, fonts, sounds, name, logo).

## Flagged for the owner
- **resvg** (MPL-2.0, a weak copyleft licence) would be needed to import SVG stickers. Not added. Say if you want it; until then stickers are PNG / GIF / WebP / APNG (through FFmpeg) plus vidzcut's own built-in shapes.
