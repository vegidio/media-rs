//! Jump to a timestamp before decoding, instead of decoding a file from the start.
//!
//! Shows: `MediaReader::seek` (land at/before the nearest keyframe) paired with
//! `Decoder::reset` (drop the decoder's buffered state after the jump). Seeking is
//! keyframe-granular, so we decode forward and skip until the exact target time — the
//! standard pattern for frame-accurate seeking on top of a keyframe seek.
//!
//! Run with: `cargo run --example seek`

use media::prelude::*;
use std::time::Duration;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/video1.mp4");
const SEEK_TO: Duration = Duration::from_secs(2);

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut reader = MediaReader::open(INPUT)?;
    let vidx = reader.best_stream(StreamKind::Video)?;
    let tb = reader.stream_time_base(vidx)?.as_f64();

    let mut decoder = reader.stream(vidx).decoder()?;

    // Seek to the keyframe at/before 2s, then reset the decoder so no pre-seek frames leak out.
    reader.seek(vidx, SEEK_TO)?;
    decoder.reset();

    let target = SEEK_TO.as_secs_f64();
    println!("Seeked toward {:?}; frames from there on:", SEEK_TO);

    let mut shown = 0;
    'outer: for packet in reader.packets() {
        let packet = packet?;
        if packet.stream_index() != vidx {
            continue;
        }

        for frame in decoder.decode(&packet)? {
            let frame = frame?;
            let secs = frame.best_effort_timestamp().map(|ts| ts as f64 * tb).unwrap_or(f64::NAN);
            // The keyframe may sit before the target; skip forward to the exact point.
            if secs + f64::EPSILON < target {
                continue;
            }
            println!("  frame @ {secs:.3}s");
            shown += 1;
            if shown == 5 {
                break 'outer;
            }
        }
    }

    Ok(())
}
