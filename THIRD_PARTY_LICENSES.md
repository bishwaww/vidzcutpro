# Third-party licences

Every library, font and asset that ships with vidzcut is listed here with its licence.
Rule (spec §0): MIT / Apache-2.0 / BSD / ISC / OFL (fonts) / CC0 only. FFmpeg is LGPL and dynamically linked. No GPL without the owner's approval.

Status (2026-10-02, end of Phase 1): **direct dependencies only**. A full check of every indirect dependency (e.g. with `cargo-deny`) is planned for Phase 7, before any release.

## Rust crates (direct dependencies)
| Crate | Version | Licence | Used in |
|---|---|---|---|
| eframe, egui | 0.36 | MIT OR Apache-2.0 | vz-ui (window + interface) |
| cosmic-text | 0.19 | MIT OR Apache-2.0 | spikes/sinhala now; vz-text later |
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

## Fonts (bundled)
| Font | Licence | Used for |
|---|---|---|
| egui's built-in fonts (Ubuntu-Light, Hack, emoji icon fonts, Noto Emoji) | OFL / Ubuntu Font Licence / MIT (as shipped inside egui) | interface (Latin) |
| _(planned, Phase 2)_ Noto Sans Sinhala, Noto Serif Sinhala, Noto Sans Tamil, Inter, Abhaya Libre | SIL OFL 1.1 | UI + video text |
| Windows' Nirmala UI (**temporary**, only read from the user's own Windows install, never copied or shipped) | Microsoft system font | Sinhala in the interface until Noto is bundled |

## Icons / artwork
| Asset | Licence | Notes |
|---|---|---|
| vidzcut logo, mark, app icon (`assets/brand/`) | Owner's own artwork | copied from the prototype |
| _(planned)_ Lucide icons | ISC | UI icons |

## Not allowed / never used
- Anything from CapCut (code, binaries, icons, effects, LUTs, stickers, templates, fonts, sounds, name, logo).
