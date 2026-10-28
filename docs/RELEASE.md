# Making a release of vidzcut

## Windows installer (unsigned)

```
powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1
```

Result: `dist\vidzcut_0.1.0_x64-setup.exe` (about 57 MB). The script makes the licence list, builds the release program and runs
`cargo packager` with `packaging\installer.json`.

One-time: `cargo install cargo-packager --locked` (it downloads the NSIS installer maker the first time it runs).

What the installer does
- Installs for the current user only (no administrator question) into `%LOCALAPPDATA%\vidzcut`, adds a Start menu entry and an uninstaller.
- Puts these next to `vidzcut.exe`: the FFmpeg DLLs, `FFMPEG-NOTICE.txt` (LGPL notice + where to get the source), `FFMPEG-LICENSE.txt`,
  `THIRD_PARTY_CRATES.txt` (all 496 Rust libraries and their licences), `THIRD_PARTY_LICENSES.md`.
- The program file has the vidzcut icon and version (`crates/vidzcut/build.rs`).
- Uninstalling removes the program; the user's settings, recordings, templates and proxies (`%APPDATA%\vidzcut`, `%LOCALAPPDATA%\vidzcut`) stay.

**Not signed.** Windows SmartScreen will say "Windows protected your PC / unknown publisher" the first time: click **More info → Run anyway**.
To sign later you need a code-signing certificate; `cargo-packager` can run a signing command (`signCommand` in `packaging/installer.json`).

Checked on 2026-10-28 on the development PC: silent install into a temporary folder, start with an empty PATH (so the program really uses its own
DLLs), import and show a video, uninstall.

Notes
- `cargo packager -c file` fails on this PC (a Windows path problem inside the tool), so the script hands it the settings as text.
- `Cargo.toml` version, `packaging\installer.json` `version` and the file name must be changed together for a new version.

## Before every release
1. `scripts\gate.ps1 -Release` prints GATE PASSED.
2. `scripts\make-licence-list.ps1` shows no "licence to check" warnings. (Known: `option-ext` is MPL-2.0, see THIRD_PARTY_LICENSES.md.)
3. The FFmpeg in `third_party\ffmpeg` is the LGPL build (`setup-windows.ps1` checks this).
4. OpenH264's licence and patent terms are confirmed with the owner (still open, see THIRD_PARTY_LICENSES.md).

## macOS (not made yet)
A Mac is needed. Plan: build an LGPL FFmpeg 9 for macOS (the Homebrew one may be GPL and must not be shipped), then
`cargo packager --formats dmg` with a config like `packaging/installer.json` (icons `icon.png`, the FFmpeg dylibs as resources). Unsigned Mac apps need
right-click → Open the first time. Hardware decoding (VideoToolbox) and the macOS build have never been run.
