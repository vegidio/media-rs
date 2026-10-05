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

/// `EveryNFrames` decodes sequentially and emits every n-th decoded frame, exercising the
/// `next_every_n` path (its one-shot seek and in-plan counter) end to end.
#[test]
fn every_n_frames_emits_sequentially_indexed_frames() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();

    // Every 10th decoded frame, in memory so we can inspect indices and pixels.
    let report = FrameExtractor::builder()
        .input(&input)
        .interval(Interval::EveryNFrames(10))
        .resolution(Resolution::Fixed(64, 36))
        .to_memory()
        .build()
        .unwrap()
        .run()
        .unwrap();

    let frames = report.frames();
    assert!(!frames.is_empty(), "expected at least one sampled frame");
    let mut last_ts = -1.0_f64;
    for (i, frame) in frames.iter().enumerate() {
        // Indices are the running emit count: 0, 1, 2, … regardless of the 10-frame stride.
        assert_eq!(frame.index() as usize, i);
        assert_eq!(frame.dimensions(), (64, 36));
        assert_eq!(frame.to_rgb_bytes().len(), 64 * 36 * 3);
        // Emitted frames advance monotonically in time.
        let t = frame.timestamp().as_secs_f64();
        assert!(t >= last_ts, "timestamps must not go backwards ({t} < {last_ts})");
        last_ts = t;
    }
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

/// Extracts one in-memory frame from `asset` at `resolution` and returns its dimensions, or
/// `None` when the assets are absent.
fn one_frame_dimensions(asset: &str, resolution: Resolution) -> Option<(u32, u32)> {
    let input = common::asset(asset);
    if !input.exists() {
        return None;
    }

    let report = FrameExtractor::builder()
        .input(input.to_str().unwrap())
        .interval(Interval::Timestamps(vec![Duration::ZERO]))
        .resolution(resolution)
        .to_memory()
        .build()
        .unwrap()
        .run()
        .unwrap();

    let frame = &report.frames()[0];
    assert_eq!(frame.to_rgb_bytes().len(), (frame.dimensions().0 * frame.dimensions().1 * 3) as usize);
    Some(frame.dimensions())
}

/// `Fit` below the source size scales the longer edge to the bound and keeps the aspect ratio,
/// for a landscape and a portrait stream.
#[test]
fn fit_scales_a_frame_down_to_the_bound() {
    // video2.mp4 is 1280×720; video1.mp4 is 676×1280 (676 * 100 / 1280 = 52.8 → 53).
    if let Some(dimensions) = one_frame_dimensions("video2.mp4", Resolution::Fit(320)) {
        assert_eq!(dimensions, (320, 180));
    }
    if let Some(dimensions) = one_frame_dimensions("video1.mp4", Resolution::Fit(100)) {
        assert_eq!(dimensions, (53, 100));
    }
}

/// `Fit` above the source size passes the frame through at its own size — never enlarged.
#[test]
fn fit_never_enlarges_a_frame() {
    if let Some(dimensions) = one_frame_dimensions("video2.mp4", Resolution::Fit(4096)) {
        assert_eq!(dimensions, (1280, 720));
    }
    if let Some(dimensions) = one_frame_dimensions("video2.mp4", Resolution::Fit(1280)) {
        assert_eq!(dimensions, (1280, 720));
    }
}

/// A `Fit` bound of 0 is a misconfiguration, refused when the extractor is built.
#[test]
fn fit_zero_is_refused() {
    let result = FrameExtractor::builder()
        .input("unused.mp4")
        .interval(Interval::Count(1))
        .resolution(Resolution::Fit(0))
        .to_memory()
        .build();

    assert!(matches!(result, Err(Error::InvalidConfig(_))));
}
