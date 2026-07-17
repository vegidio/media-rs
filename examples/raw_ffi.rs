//! Reach past the safe wrapper straight into the raw FFmpeg FFI bindings.
//!
//! Shows: `media::sys` — the escape hatch for anything the safe API doesn't wrap yet. Here we
//! call `avcodec_version`/`avformat_version` directly and decode FFmpeg's packed
//! `MAJOR.MINOR.MICRO` version integer by hand. Like `version.rs`, this doesn't use the prelude.
//!
//! Run with: `cargo run --example raw_ffi`

/// Decode FFmpeg's packed version integer (0xMMNNPP) into `major.minor.micro`.
fn parts(v: u32) -> (u32, u32, u32) {
    ((v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff)
}

fn main() {
    // SAFETY: both are argument-less C accessors returning a packed version constant.
    let (codec, format) = unsafe { (media::sys::avcodec_version(), media::sys::avformat_version()) };

    let (cmaj, cmin, cmic) = parts(codec);
    let (fmaj, fmin, fmic) = parts(format);

    println!("Raw FFI reachable via `media::sys`:");
    println!("  libavcodec  {cmaj}.{cmin}.{cmic}");
    println!("  libavformat {fmaj}.{fmin}.{fmic}");
}
