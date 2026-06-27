//! Frame-level (Tier 3) end-to-end transcode: decode → re-encode H.264 → mux, then re-open
//! the result and confirm it is a valid, decodable file.

mod common;

use media::codec::VideoEncoder;
use media::prelude::*;
use media::types::StreamKind;

/// Encode a frame's packets into the writer, tagging them with the output stream index.
fn drain_encoder(
    encoder: &mut VideoEncoder,
    writer: &mut MediaWriter,
    out_idx: usize,
    flush: bool,
    frame: Option<&Frame>,
) -> media::Result<()> {
    let iter = if flush {
        encoder.flush()?
    } else {
        encoder.encode(frame.unwrap())?
    };
    for pkt in iter {
        let mut pkt = pkt?;
        pkt.set_stream_index(out_idx);
        writer.write_packet(&mut pkt)?;
    }
    Ok(())
}

#[test]
fn transcode_video_roundtrip() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        eprintln!("no sample assets present; skipping");
        return;
    };
    let input = input.to_str().unwrap().to_owned();

    let output = common::temp("media_rs_transcode_test.mp4");
    let _ = std::fs::remove_file(&output);

    let in_video = probe(&input).unwrap().video().cloned().unwrap();
    let (in_width, in_height) = (in_video.width, in_video.height);

    // --- transcode ---------------------------------------------------------------------
    {
        let mut reader = MediaReader::open(&input).unwrap();
        let vidx = reader.best_stream(StreamKind::Video).unwrap();
        let in_tb = reader.stream_time_base(vidx).unwrap();
        let fr = reader.stream_avg_frame_rate(vidx).unwrap();

        let mut decoder = reader.stream(vidx).decoder().unwrap();
        let mut encoder = VideoEncoder::builder()
            .codec(VideoCodec::H264)
            .from_decoder(&decoder)
            .framerate(Framerate(fr))
            .time_base(in_tb)
            .preset(H264Preset::Ultrafast)
            .build()
            .unwrap();

        let mut writer = MediaWriter::create(&output).unwrap();
        let out_idx = writer.add_stream_from_encoder(&encoder).unwrap();
        writer.write_header().unwrap();

        for packet in reader.packets() {
            let packet = packet.unwrap();
            if packet.stream_index() != vidx {
                continue;
            }
            for frame in decoder.decode(&packet).unwrap() {
                let mut frame = frame.unwrap();
                if let Some(ts) = frame.best_effort_timestamp() {
                    frame.set_pts(ts);
                }
                drain_encoder(&mut encoder, &mut writer, out_idx, false, Some(&frame)).unwrap();
            }
        }
        // Flush decoder, then encoder.
        let mut tail = Vec::new();
        for frame in decoder.flush().unwrap() {
            tail.push(frame.unwrap());
        }
        for mut frame in tail {
            if let Some(ts) = frame.best_effort_timestamp() {
                frame.set_pts(ts);
            }
            drain_encoder(&mut encoder, &mut writer, out_idx, false, Some(&frame)).unwrap();
        }
        drain_encoder(&mut encoder, &mut writer, out_idx, true, None).unwrap();
        writer.write_trailer().unwrap();
    }

    // --- verify the output is a real, decodable H.264 file -----------------------------
    let info = probe(&output).unwrap();
    let video = info.video().expect("output has no video stream");
    assert_eq!(video.video_codec, Some(VideoCodec::H264));
    assert_eq!((video.width, video.height), (in_width, in_height));
    assert!(info.duration().as_secs_f64() > 0.0, "output has zero duration");

    let mut reader = MediaReader::open(&output).unwrap();
    let vidx = reader.best_stream(StreamKind::Video).unwrap();
    let mut decoder = reader.stream(vidx).decoder().unwrap();
    let mut frames = 0_u64;
    for packet in reader.packets() {
        let packet = packet.unwrap();
        if packet.stream_index() != vidx {
            continue;
        }
        for frame in decoder.decode(&packet).unwrap() {
            frame.unwrap();
            frames += 1;
        }
    }
    assert!(frames > 0, "could not decode any frames from the output");

    let _ = std::fs::remove_file(&output);
}
