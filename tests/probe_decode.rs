//! Read-path integration tests: probing and decoding the sample videos.

mod common;

use media::prelude::*;
use media::types::StreamKind;

#[test]
fn probe_reports_a_video_stream() {
    for path in common::sample_videos() {
        let p = path.to_str().unwrap();
        let info = probe(p).unwrap_or_else(|e| panic!("probe {p} failed: {e}"));

        assert!(info.stream_count() > 0, "{p}: no streams");
        let video = info.video().unwrap_or_else(|| panic!("{p}: no video stream"));
        assert!(video.width > 0 && video.height > 0, "{p}: zero dimensions");
        assert!(info.duration().as_secs_f64() > 0.0, "{p}: zero duration");
    }
}

#[test]
fn decodes_frames_consistently() {
    for path in common::sample_videos() {
        let p = path.to_str().unwrap();

        let mut reader = MediaReader::open(p).unwrap();
        let video_idx = reader.best_stream(StreamKind::Video).unwrap();
        let mut decoder = reader.stream(video_idx).decoder().unwrap();

        let mut count = 0_u64;
        let mut dims = None;
        for packet in reader.packets() {
            let packet = packet.unwrap();
            if packet.stream_index() != video_idx {
                continue;
            }
            for frame in decoder.decode(&packet).unwrap() {
                let frame = frame.unwrap();
                dims.get_or_insert((frame.width(), frame.height()));
                count += 1;
            }
        }
        // Drain buffered frames at EOF.
        for frame in decoder.flush().unwrap() {
            frame.unwrap();
            count += 1;
        }

        assert!(count > 0, "{p}: decoded no frames");
        let (w, h) = dims.unwrap();
        assert!(w > 0 && h > 0, "{p}: decoded frame had zero dimensions");
    }
}
