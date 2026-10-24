//! Shared helpers for integration tests.
//!
//! Each test binary `mod`-includes this file, so a helper unused by one binary would warn
//! there; allow dead code for the module as a whole.
#![allow(dead_code)]

use media::prelude::*;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Absolute path to an asset file under `assets/`.
pub fn asset(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join(name)
}

/// A path to `name` in the system temp directory, as a string.
pub fn temp(name: &str) -> String {
    std::env::temp_dir().join(name).to_str().unwrap().to_owned()
}

/// The sample videos, skipped gracefully if the `assets/` folder isn't present (it is
/// excluded from the published crate).
pub fn sample_videos() -> Vec<PathBuf> {
    ["video1.mp4", "video2.mp4", "video3.mp4"]
        .iter()
        .map(|n| asset(n))
        .filter(|p| p.exists())
        .collect()
}

/// The one sample that carries an audio stream (`video2.mp4`), or `None` when assets are
/// absent. Audio-path tests use this so they exercise real demux/decode/stream-copy.
pub fn audio_sample() -> Option<PathBuf> {
    let p = asset("video2.mp4");
    p.exists().then_some(p)
}

/// The audio-only sample (`audio1.mp3`), or `None` when assets are absent. Used by tests that
/// exercise the audio-only paths (format conversion, auto-encode) which the video-with-audio
/// sample can't cover as cleanly.
pub fn audio_only_sample() -> Option<PathBuf> {
    let p = asset("audio1.mp3");
    p.exists().then_some(p)
}

/// A writer whose bytes the test can read back while the `MediaWriter` owns a clone of it.
#[derive(Clone, Default)]
pub struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl SharedBuf {
    pub fn bytes(&self) -> Vec<u8> {
        self.0.lock().unwrap().clone()
    }
}

impl Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Copy every stream of `input` into `writer`, passing each packet through `rewrite` (which may also act on the
/// writer, e.g. flush it) before it's written.
pub fn remux_with(
    input: &str,
    mut writer: MediaWriter,
    mut rewrite: impl FnMut(&mut MediaWriter, Packet) -> Result<Packet>,
) -> Result<()> {
    let mut reader = MediaReader::open(input)?;
    let mut out_index = Vec::new();
    for i in 0..reader.stream_count() {
        out_index.push(writer.add_stream_copy(&reader, i)?);
    }
    writer.write_header()?;
    for packet in reader.packets() {
        let packet = packet?;
        let index = packet.stream_index();
        let mut packet = rewrite(&mut writer, packet)?;
        packet.set_stream_index(out_index[index]);
        writer.write_packet(&mut packet)?;
    }
    writer.write_trailer()
}

/// Copy every stream of `input` into `writer` unchanged.
pub fn remux(input: &str, writer: MediaWriter) -> Result<()> {
    remux_with(input, writer, |_, packet| Ok(packet))
}

/// Decode `path`'s best video stream end-to-end and count the frames.
pub fn decoded_video_frames(path: &str) -> u64 {
    let mut reader = MediaReader::open(path).unwrap();
    let vidx = reader.best_stream(StreamKind::Video).unwrap();
    let mut decoder = reader.stream(vidx).decoder().unwrap();
    let mut count = 0u64;

    for packet in reader.packets() {
        let packet = packet.unwrap();
        if packet.stream_index() != vidx {
            continue;
        }

        for frame in decoder.decode(&packet).unwrap() {
            frame.unwrap();
            count += 1;
        }
    }

    for frame in decoder.flush().unwrap() {
        frame.unwrap();
        count += 1;
    }

    count
}
