//! Frame-extraction integration tests: exercise the real seek/decode/convert/encode path
//! against the sample videos and re-decode the output to confirm it's valid.

mod common;

use media::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A `Count(n)` run writes exactly `n` image files, each a valid, non-empty JPEG.
#[test]
fn count_writes_exactly_n_valid_jpegs() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();
    let dir = std::env::temp_dir().join("media_rs_extract_count");
    let _ = std::fs::remove_dir_all(&dir);

    let report = FrameExtractor::builder()
        .input(&input)
        .interval(Interval::Count(8))
        .format(ImageFormat::Jpeg { quality: 80 })
        .output_dir(&dir)
        .build()
        .unwrap()
        .run()
        .unwrap();

    assert_eq!(report.frame_count(), 8, "expected exactly 8 frames");

    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    assert_eq!(files.len(), 8, "expected 8 files on disk");

    for path in &files {
        // rust-sak must be able to decode what we wrote back into a real image.
        let info = rust_sak::image::probe_file(path).unwrap();
        assert!(info.width > 0 && info.height > 0, "{path:?}: zero dimensions");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// In-memory output returns frames carrying the requested dimensions and raw RGB pixels.
#[test]
fn in_memory_frames_carry_pixels_and_dimensions() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();

    let report = FrameExtractor::builder()
        .input(&input)
        .interval(Interval::Count(3))
        .resolution(Resolution::Fixed(160, 90))
        .to_memory()
        .build()
        .unwrap()
        .run()
        .unwrap();

    let frames = report.frames();
    assert_eq!(frames.len(), 3);
    for (i, frame) in frames.iter().enumerate() {
        assert_eq!(frame.index() as usize, i);
        assert_eq!(frame.dimensions(), (160, 90));
        // Packed RGB24 → width * height * 3 bytes.
        assert_eq!(frame.to_rgb_bytes().len(), 160 * 90 * 3);
        // And it must encode to a non-empty PNG.
        let png = frame.encode(ImageFormat::Png).unwrap();
        assert!(!png.is_empty());
    }
}

/// The callback output delivers each frame exactly once and buffers nothing in the report.
#[test]
fn callback_receives_each_frame() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();

    // The callback must be `'static`, so share state through an `Arc<Mutex<_>>`.
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let report = FrameExtractor::builder()
        .input(&input)
        .interval(Interval::Count(4))
        .to_callback(move |frame| {
            sink.lock().unwrap().push(frame.index());
            Ok(())
        })
        .build()
        .unwrap()
        .run()
        .unwrap();

    assert_eq!(report.frame_count(), 4);
    assert!(report.frames().is_empty(), "callback output must not buffer");
    assert_eq!(*seen.lock().unwrap(), vec![0, 1, 2, 3]);
}

/// The Tier-3 iterator yields lazily and can be stopped early.
#[test]
fn tier3_iterator_can_stop_early() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();

    let mut reader = MediaReader::open(&input).unwrap();
    let vidx = reader.best_stream(StreamKind::Video).unwrap();

    let mut count = 0;
    for frame in reader.stream(vidx).sampled_at(Interval::EverySeconds(0.5)).unwrap() {
        let frame = frame.unwrap();
        assert!(!frame.to_rgb_bytes().is_empty());
        count += 1;
        if count == 2 {
            break; // stop early — the iterator should not have decoded the whole stream
        }
    }
    assert_eq!(count, 2);
}

/// A range restricts extraction to the requested window.
#[test]
fn range_limits_the_window() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();
    let full = probe(&input).unwrap().duration().as_secs_f64();
    if full < 3.0 {
        return; // too short to exercise a sub-range
    }

    let report = FrameExtractor::builder()
        .input(&input)
        .interval(Interval::EverySeconds(0.5))
        .range(Duration::from_secs(1)..=Duration::from_secs(2))
        .to_memory()
        .build()
        .unwrap()
        .run()
        .unwrap();

    // 1s..=2s at 0.5s spacing → timestamps 1.0, 1.5, 2.0 (3 frames), and every frame must
    // fall inside the window.
    assert!(report.frame_count() >= 2 && report.frame_count() <= 3);
    for frame in report.frames() {
        let t = frame.timestamp().as_secs_f64();
        assert!((0.9..=2.1).contains(&t), "frame at {t}s outside the range");
    }
}
