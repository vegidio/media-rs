//! Decode raw video frames yourself (the Tier 3 read path), without any encoding.
//!
//! Shows: `MediaReader::open`, `best_stream`, `stream().decoder()`, iterating `packets()`,
//! `Decoder::decode`/`flush`, and inspecting each decoded `Frame`.
//!
//! Run with: `cargo run --example decode_frames`

use media::prelude::*;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/video1.mp4");

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut reader = MediaReader::open(INPUT)?;
    let vidx = reader.best_stream(StreamKind::Video)?;
    let mut decoder = reader.stream(vidx).decoder()?;

    println!("Decoding stream {vidx}: {}x{} {:?}", decoder.width(), decoder.height(), decoder.pixel_format());

    let mut frames = 0_u64;
    for packet in reader.packets() {
        let packet = packet?;
        if packet.stream_index() != vidx {
            continue; // skip audio / other streams
        }

        for frame in decoder.decode(&packet)? {
            let frame = frame?;
            if frames == 0 {
                println!(
                    "First frame: {}x{} {:?}, ts {:?}",
                    frame.width(),
                    frame.height(),
                    frame.pixel_format(),
                    frame.best_effort_timestamp()
                );
            }

            frames += 1;
        }
    }

    // Drain any frames the decoder still holds at end of stream.
    for frame in decoder.flush()? {
        frame?;
        frames += 1;
    }

    println!("Decoded {frames} frames total");
    Ok(())
}
