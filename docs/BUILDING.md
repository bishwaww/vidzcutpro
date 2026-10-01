# Building vidzcut

Last tested: 2026-10-02 on Windows 11 (i5-13420H, RTX 4050 + Intel UHD). macOS is **not tested yet** (no Mac available): see the macOS section.

## Windows — the short way

1. Open the project folder `D:\Projects\vidzcutpro` in PowerShell.
2. First time only: `powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1`
   It checks for Rust, Microsoft C++ Build Tools, LLVM and the LGPL FFmpeg, and **asks before each download** (add `-Yes` to say yes to all). It also sets the user variable `LIBCLANG_PATH`.
3. Every time: double-click **`run-vidzcut.bat`** (or run `powershell -ExecutionPolicy Bypass -File scripts\run.ps1`). The first build takes several minutes, later ones are fast.

## What gets installed (and why)

| Tool | Why | Installed by |
|---|---|---|
| Rust (stable, MSVC) | the programming language | `winget install Rustlang.Rustup` |
| Microsoft C++ Build Tools | Rust needs Microsoft's linker on Windows | `winget install Microsoft.VisualStudio.2022.BuildTools` (C++ workload) |
| LLVM | lets the FFmpeg bindings read FFmpeg's header files | `winget install LLVM.LLVM` |
| FFmpeg n9.0, LGPL, shared | decoding / encoding. Unpacked to `third_party\ffmpeg` (git-ignored) | downloaded by the setup script from github.com/BtbN/FFmpeg-Builds |

`.cargo\config.toml` points `FFMPEG_DIR` at `third_party\ffmpeg`. `crates\vz-media\build.rs` copies the FFmpeg DLLs next to the programs Cargo builds, so nothing needs to be added to PATH.

**The FFmpeg build must be LGPL.** The setup script refuses a build that contains `--enable-gpl`, and a unit test in `vz-media` fails if `libx264` or `libx265` are present.

## Developer commands

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
cargo run --release -p vidzcut
```

Developer switches (environment variables) for checking the screen without a person:
`VIDZCUT_START=editor`, `VIDZCUT_LAYOUT=default|player_right|big_timeline`, `VIDZCUT_CATEGORY=0..9`, `VIDZCUT_SHOT=path.png` (save a screenshot, then quit), `VIDZCUT_CONFIG_DIR=…` and `VIDZCUT_LOG_DIR=…` (use temporary folders so the real settings and logs are untouched).

## Where vidzcut keeps its files (Windows)

- Settings (panel sizes, layout): `%APPDATA%\vidzcut\config\ui_settings.json`
- Log file: `%LOCALAPPDATA%\vidzcut\data\logs\vidzcut.log`

## macOS (planned, not yet tested)

1. Install Xcode command line tools: `xcode-select --install`.
2. Install Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`.
3. FFmpeg 9 development files + `pkg-config`. For *building and testing*, `brew install ffmpeg pkg-config` works; **the Homebrew build may be GPL, so it is not allowed in a release.** For release a LGPL build will be made by a script (Phase 7).
4. `cargo run --release -p vidzcut`.

`.github/workflows/build.yml` builds and tests on Windows and macOS for every push **once the project is on GitHub** (GitHub account/repo needed: owner decision). That is how macOS will be checked without a Mac.

## Known gaps

- macOS path untested; installers (Windows + macOS) come in Phase 7.
- The setup script has been tested on a machine where everything was already installed; the "install what is missing" branches have not been exercised end-to-end.
