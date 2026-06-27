//! Build script for `media-rs`.
//!
//! At build time this script obtains the prebuilt **static** FFmpeg libraries (and the
//! codec/support libraries they depend on) for the current target, links them statically
//! into the crate, and generates the raw FFI bindings from the bundled FFmpeg headers via
//! the `wrapper.h` umbrella header.
//!
//! Binaries come from: https://github.com/vegidio/binaries-ffmpeg/releases
//!
//! The binaries are normally downloaded from the pinned release and cached under the
//! build's `OUT_DIR`. To build offline (or against a custom build of FFmpeg), set the
//! `MEDIA_BINARIES_DIR` environment variable to a directory that contains `include/` and
//! `lib/` subdirectories laid out like the release archives.
//!
//! ## Linking
//!
//! The bundled set of dependency `.a` files differs per platform/architecture (e.g.
//! x264/x265/vpx are absent on windows-arm64; nvcodec/vaapi/qsv vary). Rather than
//! hardcode a per-target list, the script *discovers* every `lib*.a` in `lib/` and links
//! them all, emitting the FFmpeg core libraries first in dependency order and the rest
//! afterwards. On GNU linkers the whole set is wrapped in `--start-group`/`--end-group`
//! to resolve the circular references between FFmpeg and its dependencies.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Version of the `binaries-ffmpeg` release to download.
const VERSION: &str = "26.6.0";

/// The eight FFmpeg core libraries, listed dependents-before-dependencies so that a
/// single-pass linker resolves them. Any other `.a` found in `lib/` (third-party codecs
/// and support libraries) is linked after these.
const FFMPEG_CORE_LIBS: &[&str] = &[
    "avdevice",
    "avfilter",
    "avformat",
    "avcodec",
    "postproc",
    "swscale",
    "swresample",
    "avutil",
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-env-changed=MEDIA_BINARIES_DIR");

    let binaries_dir = locate_binaries();
    let lib_dir = binaries_dir.join("lib");
    let include_dir = binaries_dir.join("include");

    emit_link_directives(&lib_dir);
    generate_bindings(&include_dir);
}

/// Returns the directory containing the `include/` and `lib/` subdirectories, either from
/// the `MEDIA_BINARIES_DIR` override or by downloading + extracting the pinned release.
fn locate_binaries() -> PathBuf {
    if let Ok(dir) = env::var("MEDIA_BINARIES_DIR") {
        let dir = PathBuf::from(dir);
        assert!(
            dir.join("include").is_dir() && dir.join("lib").is_dir(),
            "MEDIA_BINARIES_DIR ({}) must contain `include/` and `lib/` subdirectories",
            dir.display()
        );
        return dir;
    }

    download_and_extract()
}

/// Downloads the static archive for the current target into `OUT_DIR` and extracts it.
/// Extraction is skipped if a previous build already populated the cache directory.
fn download_and_extract() -> PathBuf {
    let archive = archive_name();
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let cache_dir = out_dir.join(format!("binaries-ffmpeg-{VERSION}"));

    // Idempotent: a fully extracted cache from a previous build is reused as-is.
    if cache_dir.join("lib").is_dir() && cache_dir.join("include").is_dir() {
        return cache_dir;
    }

    let url = format!(
        "https://github.com/vegidio/binaries-ffmpeg/releases/download/{VERSION}/{archive}"
    );
    eprintln!("media-rs: downloading {url}");

    let bytes = download(&url);
    extract_zip(&bytes, &cache_dir);
    cache_dir
}

/// Maps the Cargo target triple components to the release archive file name,
/// e.g. `static_osx_arm64.zip`.
fn archive_name() -> String {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();

    let os = match target_os.as_str() {
        "linux" => "linux",
        "macos" => "osx",
        "windows" => "windows",
        other => panic!("media-rs: unsupported target OS `{other}`"),
    };

    let arch = match target_arch.as_str() {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => panic!("media-rs: unsupported target architecture `{other}`"),
    };

    format!("static_{os}_{arch}.zip")
}

/// Downloads the given URL into memory, following redirects.
fn download(url: &str) -> Vec<u8> {
    let mut reader = ureq::get(url)
        .call()
        .unwrap_or_else(|e| panic!("media-rs: failed to download {url}: {e}"))
        .into_body()
        .into_reader();

    let mut bytes = Vec::new();
    io::copy(&mut reader, &mut bytes)
        .unwrap_or_else(|e| panic!("media-rs: failed to read response body from {url}: {e}"));
    bytes
}

/// Extracts a zip archive (held entirely in memory) into `dest`.
fn extract_zip(bytes: &[u8], dest: &Path) {
    let reader = io::Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(reader).expect("media-rs: invalid zip archive");

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).expect("media-rs: corrupt zip entry");
        let Some(rel_path) = entry.enclosed_name() else {
            continue; // skip unsafe / absolute paths
        };
        let out_path = dest.join(rel_path);

        if entry.is_dir() {
            fs::create_dir_all(&out_path).unwrap();
            continue;
        }

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut out_file = fs::File::create(&out_path)
            .unwrap_or_else(|e| panic!("media-rs: cannot create {}: {e}", out_path.display()));
        io::copy(&mut entry, &mut out_file).expect("media-rs: failed to extract file");
    }
}

/// Tells Cargo/rustc where the static libraries live and which ones to link, including
/// the C++ runtime and system libraries the codecs depend on.
fn emit_link_directives(lib_dir: &Path) {
    println!("cargo:rustc-link-search=native={}", lib_dir.display());

    let libs = discover_link_order(lib_dir);
    assert!(
        !libs.is_empty(),
        "media-rs: no `lib*.a` static libraries found in {}",
        lib_dir.display()
    );

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    // GNU linkers (Linux, and the *-pc-windows-gnu toolchain) resolve symbols in a single
    // pass, so the circular references between FFmpeg and its dependencies require a link
    // group. The macOS linker (ld64) is multi-pass and needs no grouping.
    let use_link_group = target_os == "linux" || (target_os == "windows" && target_env == "gnu");

    if use_link_group {
        println!("cargo:rustc-link-arg=-Wl,--start-group");
        for lib in &libs {
            println!("cargo:rustc-link-arg=-l{lib}");
        }
        println!("cargo:rustc-link-arg=-Wl,--end-group");
    } else {
        for lib in &libs {
            println!("cargo:rustc-link-lib=static={lib}");
        }
    }

    emit_system_libs(&target_os);
}

/// Discovers every `lib*.a` in `lib_dir` and returns the link names (without the `lib`
/// prefix and `.a` suffix) ordered FFmpeg-core-first, dependencies after.
fn discover_link_order(lib_dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(lib_dir)
        .unwrap_or_else(|e| panic!("media-rs: cannot read {}: {e}", lib_dir.display()))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let name = path.file_name()?.to_str()?;
            // Static archives are named `lib<name>.a` on every supported platform
            // (including the MinGW Windows builds).
            let stem = name.strip_prefix("lib")?.strip_suffix(".a")?;
            (!stem.is_empty()).then(|| stem.to_string())
        })
        .collect();

    let mut ordered = Vec::with_capacity(found.len());

    // FFmpeg core libraries first, in dependency order.
    for core in FFMPEG_CORE_LIBS {
        if let Some(pos) = found.iter().position(|l| l == core) {
            ordered.push(found.swap_remove(pos));
        }
    }

    // Remaining third-party dependencies, in a stable (sorted) order.
    found.sort();
    ordered.extend(found);
    ordered
}

/// Emits the system libraries / frameworks that the FFmpeg static libs reference but that
/// are provided by the OS rather than bundled in the archive.
fn emit_system_libs(target_os: &str) {
    match target_os {
        "macos" => {
            println!("cargo:rustc-link-lib=dylib=c++");
            println!("cargo:rustc-link-lib=dylib=z");
            println!("cargo:rustc-link-lib=dylib=iconv");
            for framework in [
                "CoreFoundation",
                "CoreServices",
                "CoreMedia",
                "CoreVideo",
                "CoreAudio",
                "AudioToolbox",
                "VideoToolbox",
                "AVFoundation",
                "Foundation",
                "Metal",
                "Security",
            ] {
                println!("cargo:rustc-link-lib=framework={framework}");
            }
        }
        "linux" => {
            println!("cargo:rustc-link-lib=dylib=stdc++");
            println!("cargo:rustc-link-lib=dylib=m");
            println!("cargo:rustc-link-lib=dylib=pthread");
            println!("cargo:rustc-link-lib=dylib=dl");
            println!("cargo:rustc-link-lib=dylib=z");
        }
        "windows" => {
            // The release `.a` archives are GNU-style; building for Windows therefore
            // expects the `*-pc-windows-gnu` toolchain.
            println!("cargo:rustc-link-lib=dylib=stdc++");
            println!("cargo:rustc-link-lib=dylib=pthread");
            for lib in [
                "bcrypt", "ws2_32", "secur32", "ole32", "oleaut32", "user32", "gdi32",
                "strmiids", "mfplat", "mfuuid", "vfw32",
            ] {
                println!("cargo:rustc-link-lib=dylib={lib}");
            }
        }
        _ => {}
    }
}

/// Generates raw FFI bindings from `wrapper.h` into `OUT_DIR/bindings.rs`.
fn generate_bindings(include_dir: &Path) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", include_dir.display()))
        // Keep the output focused on the FFmpeg surface.
        .allowlist_function("(av|avcodec|avformat|avfilter|avdevice|sws|swr|swscale|swresample|postproc|pp)_.*")
        .allowlist_function("avformat_.*")
        .allowlist_function("avcodec_.*")
        .allowlist_function("avfilter_.*")
        .allowlist_function("avdevice_.*")
        .allowlist_function("avio_.*")
        .allowlist_function("av_.*")
        .allowlist_function("sws_.*")
        .allowlist_function("swr_.*")
        .allowlist_function("postproc_.*")
        .allowlist_type("(AV|SWS|SWR|FF).*")
        .allowlist_type("av.*")
        .allowlist_type("sws.*")
        .allowlist_type("swr.*")
        .allowlist_type("pp.*")
        .allowlist_var("(AV|SWS|SWR|FF).*")
        .allowlist_var("av.*")
        .generate_comments(false)
        .layout_tests(false)
        .generate()
        .expect("media-rs: failed to generate bindings from wrapper.h");

    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("media-rs: failed to write bindings.rs");
}
