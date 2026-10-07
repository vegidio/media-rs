//! Cut decoded audio at an exact time with a public `AudioFilter`, then encode what is left to AAC.
//!
//! Decoded audio comes in frames of hundreds or thousands of samples, so skipping whole frames can only cut at a frame
//! boundary. `atrim` cuts inside a frame, at the sample.
//!
//! Shows: `AudioFilter::new` from a decoder, `atrim` through `AudioFilterChain::raw`, and feeding the result to an
//! `AudioEncoder`.
//!
//! Run with: `cargo run --example audio_trim`

use media::prelude::*;
use std::time::Duration;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/audio1.mp3");
const START: Duration = Duration::from_millis(10_510);

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut reader = MediaReader::open(INPUT)?;
    let index = reader.best_stream(StreamKind::Audio)?;
    let time_base = reader.stream_time_base(index)?;
    let mut decoder = reader.stream(index).decoder()?;

    // Seek near the cut, then let `atrim` drop everything before it, sample by sample.
    reader.seek(index, START)?;
    decoder.reset();
    let chain = AudioFilterChain::raw(format!("atrim=start={}", START.as_secs_f64()));
    let mut trim = AudioFilter::new(&decoder, time_base, &chain)?;
    let mut encoder = AudioEncoder::builder().codec(AudioCodec::Aac).from_decoder(&decoder).build()?;

    let (mut first, mut samples, mut packets) = (None, 0u64, 0usize);
    for packet in reader.packets() {
        let packet = packet?;
        if packet.stream_index() != index {
            continue;
        }
        for frame in decoder.decode(&packet)?.collect::<media::Result<Vec<_>>>()? {
            let mut frame = frame;
            // `atrim` reads each frame's timestamp.
            frame.set_pts(frame.best_effort_timestamp().unwrap_or(0));
            for kept in trim.filter(frame)? {
                first.get_or_insert(kept.pts().unwrap_or(0) as f64 * time_base.as_f64());
                samples += u64::from(kept.sample_count());
                packets += encoder.encode(&kept)?.len();
            }
        }
    }
    packets += encoder.flush()?.len();

    println!(
        "cut at {:.3} s; the first kept sample is at {:.4} s",
        START.as_secs_f64(),
        first.unwrap_or_default()
    );
    println!("{samples} samples kept, {packets} AAC packets");

    Ok(())
}
