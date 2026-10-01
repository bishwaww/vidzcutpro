# Building vidzcut

Status: **draft** — written in Phase 0 before any code exists. It will be tested and finished in step 0f.

## What this PC has (checked 2026-10-01)

| Tool | Status |
|---|---|
| Git 2.55 | installed |
| Node, Python 3.12 | installed (not needed for vidzcut itself) |
| winget (Windows package manager) | installed |
| **Rust (rustc, cargo)** | **missing** |
| **Microsoft C++ Build Tools (MSVC)** | **missing** |
| **LLVM / clang** (needed by FFmpeg bindings) | **missing** |
| **FFmpeg (LGPL, shared)** | **missing** |
| GPU | NVIDIA RTX 4050 Laptop + Intel UHD (so NVENC and QuickSync can both be tested) |
| RAM / disk | 15.6 GB RAM, ~100 GB free on D: |

## Windows setup (planned, needs owner OK for each download)

1. **Rust** — `winget install Rustlang.Rustup`, then `rustup default stable`. (MIT/Apache, ~500 MB installed.)
2. **C++ build tools** — `winget install Microsoft.VisualStudio.2022.BuildTools` with the "Desktop development with C++" workload. (Microsoft, free, ~3–6 GB. Needed to link Rust programs on Windows.)
3. **LLVM** — `winget install LLVM.LLVM` (for `bindgen`, which reads FFmpeg's C headers).
4. **FFmpeg (LGPL, shared, with headers + import libs)** — a pre-built "lgpl-shared" Windows build, unpacked to `D:\Projects\vidzcutpro\third_party\ffmpeg\`. Environment variables: `FFMPEG_DIR`, `LIBCLANG_PATH`. The DLLs are copied next to `vidzcut.exe`.
5. `scripts\setup-windows.ps1` will do steps 1–4 and 5's environment variables in one go (asking before each download).
6. Build + run: `cargo run --release -p vidzcut`.

## macOS (planned)

Xcode command line tools, `brew install rustup llvm pkg-config ffmpeg-lgpl` (or the same LGPL build via a script), `cargo run --release -p vidzcut`. Universal (Apple Silicon + Intel) `.app`/`.dmg` via `cargo-packager`. I cannot run macOS here, so a GitHub Actions workflow (`.github/workflows/build.yml`) will build Windows + macOS on every push once there is a GitHub repository.

## Quality gate (before every commit)

```
cargo fmt --all
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo build --release
```
