//! Copies the FFmpeg DLLs next to the programs Cargo builds, so `cargo run` and `cargo test`
//! find them without anyone touching PATH. (Windows only; other systems use installed libraries.)

use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=FFMPEG_DIR");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let Some(dir) = std::env::var_os("FFMPEG_DIR") else {
        return;
    };
    let bin = Path::new(&dir).join("bin");
    let Ok(entries) = std::fs::read_dir(&bin) else {
        println!(
            "cargo:warning=FFmpeg DLLs not found in {} (run scripts/setup-windows.ps1)",
            bin.display()
        );
        return;
    };
    // OUT_DIR is target/<profile>/build/<crate>-<hash>/out; the programs live two levels above `build`.
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    let Some(profile_dir) = out.ancestors().nth(3) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
        {
            let name = entry.file_name();
            for target in [profile_dir.to_path_buf(), profile_dir.join("deps")] {
                let _ = std::fs::create_dir_all(&target);
                let _ = std::fs::copy(&path, target.join(&name));
            }
        }
    }
}
