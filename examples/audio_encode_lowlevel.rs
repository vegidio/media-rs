//! The full frame-level audio pipeline, wired by hand: read -> decode -> DSP -> encode -> mux,
//! with a custom sample-level effect in the middle.
//!
//! Shows: `MediaReader` + `Decoder` feeding an `AudioEncoder` (built with `from_decoder`) into
//! a `MediaWriter`, direct PCM access via `Frame::samples_mut()` / `SampleBuffer` to halve the
//! gain, and flushing the encoder so the final samples aren't truncated. The `AudioEncoder`
//! resamples and buffers internally, so `encode()` returns a `Vec<Packet>`.
//!
//! Run with: `cargo run --example audio_encode_lowlevel`

use media::prelude::*;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/audio1.mp3");
const OUTPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/temp_audio_dsp.m4a");

/// Halve every sample in place — a trivial custom DSP effect over raw PCM.
fn halve_gain(frame: &mut Frame) {
    match frame.samples_mut() {
        SampleBuffer::Fltp(planes) => {
            for plane in planes {
                for s in plane.iter_mut() {
                    *s *= 0.5;
                }
            }
        }
        SampleBuffer::S16(samples) => {
            for s in samples.iter_mut() {
                *s /= 2;
            }
        }
        _ => {} // other formats left untouched for brevity
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut reader = MediaReader::open(INPUT)?;
    let aidx = reader.best_stream(StreamKind::Audio)?;

    let mut decoder = reader.stream(aidx).decoder()?;
    let mut encoder = AudioEncoder::builder()
        .codec(AudioCodec::Aac)
        .from_decoder(&decoder) // inherit sample rate + channel layout
        .bitrate(Bitrate::kbps(128))
        .build()?;

    let mut writer = MediaWriter::create(OUTPUT)?;
    let out_idx = writer.add_stream_from_encoder(&encoder)?;
    writer.write_header()?;

    for packet in reader.packets() {
        let packet = packet?;
        if packet.stream_index() != aidx {
            continue;
        }
        for frame in decoder.decode(&packet)? {
            let mut frame = frame?;
            halve_gain(&mut frame);
            for mut pkt in encoder.encode(&frame)? {
                pkt.set_stream_index(out_idx);
                writer.write_packet(&mut pkt)?;
            }
        }
    }

    // Flush the encoder (drains its resampler tail and FIFO), then finalise the file.
    for mut pkt in encoder.flush()? {
        pkt.set_stream_index(out_idx);
        writer.write_packet(&mut pkt)?;
    }
    writer.write_trailer()?;

    let info = probe(OUTPUT)?;
    if let Some(a) = info.audio() {
        println!("Wrote {OUTPUT}: {:?}, {:.2}s", a.audio_codec, info.duration().as_secs_f64());
    }
    Ok(())
}
