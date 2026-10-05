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
fn probe_reports_the_average_frame_rate() {
    // `ffprobe -show_entries stream=avg_frame_rate` gives 24/1 for every sample's video stream.
    for path in common::sample_videos() {
        let p = path.to_str().unwrap();
        let info = probe(p).unwrap();
        assert_eq!(info.video().unwrap().frame_rate, Some(Framerate::fps(24)), "{p}");
    }

    // ...and 0/0 for the audio streams.
    if let Some(path) = common::audio_sample() {
        let info = probe(path.to_str().unwrap()).unwrap();
        assert_eq!(info.audio().unwrap().frame_rate, None);
    }
    if let Some(path) = common::audio_only_sample() {
        let info = probe(path.to_str().unwrap()).unwrap();
        assert!(info.streams().iter().all(|s| s.frame_rate.is_none()));
    }
}

#[test]
fn probe_reports_a_fractional_frame_rate() {
    // `ntsc.mp4` is 1 s of 64x64 `testsrc` at 30000/1001, which `ffprobe` reports as its `avg_frame_rate`.
    let path = common::asset("ntsc.mp4");
    if !path.exists() {
        return;
    }

    let frame_rate = probe(path.to_str().unwrap()).unwrap().video().unwrap().frame_rate.unwrap();

    assert_eq!(frame_rate, Framerate::ratio(30000, 1001));
    assert!((frame_rate.as_f64() - 29.97).abs() < 0.01, "{frame_rate:?}");
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
