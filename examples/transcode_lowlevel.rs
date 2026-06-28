//! The full frame-level pipeline, wired by hand: read -> decode -> re-encode -> mux.
//!
//! Shows: `MediaReader` + `Decoder` feeding a `VideoEncoder` (built with `from_decoder` and
//! the input's `time_base`) into a `MediaWriter`, tagging each `Packet` with its output
//! stream index, and — critically — flushing both the decoder and the encoder so the final
//! frames aren't truncated.
//!
//! This is the canonical `VideoEncoder` demo: use it when the high-level `transcode()` API
//! doesn't give you the control you need.
//!
//! Run with: `cargo run --example transcode_lowlevel`

use media::prelude::*;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/video1.mp4");
const OUTPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/temp_lowlevel.mp4");

/// Push a frame's (or the flush's) packets into the writer, tagged with the output stream.
fn drain_encoder(
    encoder: &mut VideoEncoder,
    writer: &mut MediaWriter,
    out_idx: usize,
    frame: Option<&Frame>,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let iter = match frame {
        Some(f) => encoder.encode(f)?,
        None => encoder.flush()?,
    };
    for pkt in iter {
        let mut pkt = pkt?;
        pkt.set_stream_index(out_idx);
        writer.write_packet(&mut pkt)?;
    }
    Ok(())
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut reader = MediaReader::open(INPUT)?;
    let vidx = reader.best_stream(StreamKind::Video)?;
    let in_tb = reader.stream_time_base(vidx)?;
    let fr = reader.stream_avg_frame_rate(vidx)?;

    let mut decoder = reader.stream(vidx).decoder()?;
    let mut encoder = VideoEncoder::builder()
        .codec(VideoCodec::H264)
        .from_decoder(&decoder) // inherit resolution / pixel format / frame rate
        .framerate(Framerate(fr))
        .time_base(in_tb)
        .preset(H264Preset::Ultrafast)
        .build()?;

    let mut writer = MediaWriter::create(OUTPUT)?;
    let out_idx = writer.add_stream_from_encoder(&encoder)?;
    writer.write_header()?;

    let mut frames = 0_u64;
    for packet in reader.packets() {
        let packet = packet?;
        if packet.stream_index() != vidx {
            continue;
        }
        for frame in decoder.decode(&packet)? {
            let mut frame = frame?;
            // Re-stamp the frame with the decoder's best timestamp before encoding.
            if let Some(ts) = frame.best_effort_timestamp() {
                frame.set_pts(ts);
            }
            drain_encoder(&mut encoder, &mut writer, out_idx, Some(&frame))?;
            frames += 1;
        }
    }

    // Flush the decoder, encode its trailing frames, then flush the encoder.
    let mut tail = Vec::new();
    for frame in decoder.flush()? {
        tail.push(frame?);
    }
    for mut frame in tail {
        if let Some(ts) = frame.best_effort_timestamp() {
            frame.set_pts(ts);
        }
        drain_encoder(&mut encoder, &mut writer, out_idx, Some(&frame))?;
        frames += 1;
    }
    drain_encoder(&mut encoder, &mut writer, out_idx, None)?; // flush the encoder
    writer.write_trailer()?;

    println!("Wrote {OUTPUT} ({frames} frames re-encoded to H.264)");

    // Re-open the result to confirm it's a real, decodable file.
    let info = probe(OUTPUT)?;
    if let Some(v) = info.video() {
        println!(
            "Verified: {}x{} {:?}, {:.2}s",
            v.width,
            v.height,
            v.video_codec,
            info.duration().as_secs_f64()
        );
    }
    Ok(())
}
