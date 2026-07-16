//! Locating / downloading the prebuilt FFmpeg static archives.
//!
//! This duplicates the small download+extract logic from the crate's `build.rs` because a build script cannot be
//! imported as a library. Unlike `build.rs`, the target is the *host* (we generate bindings for the machine we run on),
//! so the OS/arch come from `std::env::consts` rather than the `CARGO_CFG_TARGET_*` variables.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Default `binaries-ffmpeg` release to download. Reads the repo-root `ffmpeg-version.txt`, the single source of truth
/// shared with `build.rs`, via `include_str!` (path relative to this file). `.trim()` tolerates a trailing newline.
pub fn version() -> &'static str {
    include_str!("../../ffmpeg-version.txt").trim()
}

/// The Rust `target_os` value for the current host (used to label merged bindings).
pub fn host_os_label() -> &'static str {
    match env::consts::OS {
        "macos" => "macos",
        "linux" => "linux",
        "windows" => "windows",
        other => panic!("xtask: unsupported host OS `{other}`"),
    }
}

/// Maps the host OS/arch to the release archive file name, e.g. `static_osx_arm64.zip`.
fn host_archive_name() -> String {
    let os = match env::consts::OS {
        "linux" => "linux",
        "macos" => "osx",
        "windows" => "windows",
        other => panic!("xtask: unsupported host OS `{other}`"),
    };
    let arch = match env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => panic!("xtask: unsupported host architecture `{other}`"),
    };
    format!("static_{os}_{arch}.zip")
}

/// Returns a directory containing `include/` and `lib/`, from `MEDIA_BINARIES_DIR` or by
/// downloading + extracting the given release for the host.
pub fn locate_binaries(version: &str) -> PathBuf {
    if let Ok(dir) = env::var("MEDIA_BINARIES_DIR") {
        let dir = PathBuf::from(dir);
        assert!(
            dir.join("include").is_dir() && dir.join("lib").is_dir(),
            "MEDIA_BINARIES_DIR ({}) must contain `include/` and `lib/` subdirectories",
            dir.display()
        );
        return dir;
    }

    let archive = host_archive_name();
    let cache_root = env::var("XTASK_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::temp_dir().join("media-rs-xtask"));
    let cache_dir = cache_root.join(format!("binaries-ffmpeg-{version}"));

    if cache_dir.join("lib").is_dir() && cache_dir.join("include").is_dir() {
        return cache_dir;
    }

    let url = format!("https://github.com/vegidio/binaries-ffmpeg/releases/download/{version}/{archive}");
    eprintln!("xtask: downloading {url}");
    let bytes = download(&url);
    extract_zip(&bytes, &cache_dir);
    cache_dir
}

fn download(url: &str) -> Vec<u8> {
    let mut reader = ureq::get(url)
        .call()
        .unwrap_or_else(|e| panic!("xtask: failed to download {url}: {e}"))
        .into_body()
        .into_reader();

    let mut bytes = Vec::new();
    io::copy(&mut reader, &mut bytes).unwrap_or_else(|e| panic!("xtask: failed to read response body from {url}: {e}"));
    bytes
}

fn extract_zip(bytes: &[u8], dest: &Path) {
    let reader = io::Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(reader).expect("xtask: invalid zip archive");

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).expect("xtask: corrupt zip entry");
        let Some(rel_path) = entry.enclosed_name() else {
            continue;
        };
        let out_path = dest.join(rel_path);

        if entry.is_dir() {
            fs::create_dir_all(&out_path).unwrap();
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut out_file =
            fs::File::create(&out_path).unwrap_or_else(|e| panic!("xtask: cannot create {}: {e}", out_path.display()));
        io::copy(&mut entry, &mut out_file).expect("xtask: failed to extract file");
    }
}
