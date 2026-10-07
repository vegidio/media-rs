//! Stream a file as fragmented MP4 into any `Write`, one fragment at a time: what a Media Source
//! Extensions player is fed.
//!
//! Shows: `MediaWriter::builder()` with `writer` and `fragmented_mp4`, and `flush` before each
//! video keyframe so every fragment reaches the writer as soon as it is complete.
//!
//! Run with: `cargo run --example remux_fmp4`

use media::prelude::*;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/video2.mp4");

/// A writer that counts the bytes it receives, shared with `main` so it can read the count back.
#[derive(Clone, Default)]
struct Counter(Arc<Mutex<usize>>);

impl Counter {
    /// The bytes received so far.
    fn total(&self) -> usize {
        *self.0.lock().unwrap()
    }
}

impl Write for Counter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        *self.0.lock().unwrap() += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut reader = MediaReader::open(INPUT)?;
    let sink = Counter::default();
    let mut writer = MediaWriter::builder()
        .writer(sink.clone())
        .fragmented_mp4()
        .build()?;

    let mut out_index = Vec::with_capacity(reader.stream_count());
    for i in 0..reader.stream_count() {
        out_index.push(writer.add_stream_copy(&reader, i)?);
    }
    let video = reader.best_stream(StreamKind::Video)?;

    // The init segment (`ftyp` + `moov`) goes first; flush it out so it can be appended on its own.
    writer.write_header()?;
    writer.flush()?;
    let mut handed_over = sink.total();
    println!("init segment: {handed_over} bytes");

    // Flushing before each keyframe closes the fragment before it and pushes it into the writer.
    let mut fragments = 0;
    for packet in reader.packets() {
        let mut packet = packet?;
        if packet.stream_index() == video && packet.is_keyframe() {
            writer.flush()?;
            let total = sink.total();
            if total > handed_over {
                fragments += 1;
                println!("fragment {fragments}: {} bytes", total - handed_over);
                handed_over = total;
            }
        }
        packet.set_stream_index(out_index[packet.stream_index()]);
        writer.write_packet(&mut packet)?;
    }

    // The trailer writes the last fragment.
    writer.write_trailer()?;
    let total = sink.total();
    println!("fragment {}: {} bytes", fragments + 1, total - handed_over);
    println!("{total} bytes in all");

    Ok(())
}
