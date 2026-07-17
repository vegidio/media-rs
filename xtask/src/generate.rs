//! `xtask generate` — run bindgen over `wrapper.h` for the current host.
//!
//! The wrapper is SDK-guarded, so bindgen emits exactly the FFmpeg core plus whatever
//! hardware backends this host can parse (e.g. VideoToolbox + Vulkan on macOS; D3D on
//! Windows; VAAPI/VDPAU/CUDA/QSV/OpenCL on Linux when the matching `-dev` packages are
//! installed). The per-host outputs are later unioned by `xtask merge`.

use std::path::{Path, PathBuf};

use crate::binaries;

pub fn run(out: Option<PathBuf>, version: &str) {
    let bin = binaries::locate_binaries(version);
    let include = bin.join("include");

    // `wrapper.h` lives at the repository root, one level up from this xtask crate.
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask: no parent dir").to_path_buf();
    let wrapper = repo_root.join("wrapper.h");

    let bindings = bindgen::Builder::default()
        .header(wrapper.to_string_lossy())
        .clang_arg(format!("-I{}", include.display()))
        .allowlist_function("(av|avcodec|avformat|avfilter|avdevice|sws|swr|swscale|swresample|postproc|pp)_.*")
        .allowlist_function("avio_.*")
        .allowlist_function("av_.*")
        .allowlist_function("sws_.*")
        .allowlist_function("swr_.*")
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
        .expect("xtask: bindgen failed to generate bindings from wrapper.h");

    let out = out.unwrap_or_else(|| Path::new(".").join(format!("bindings-{}.rs", binaries::host_os_label())));
    bindings
        .write_to_file(&out)
        .unwrap_or_else(|e| panic!("xtask: failed to write {}: {e}", out.display()));
    eprintln!("xtask: wrote {} (host {})", out.display(), binaries::host_os_label());
}
