//! Tier 1 (one-liner) and Tier 2 (builder) transcode tests, verified by re-probing output.

mod common;

use media::prelude::*;
use media::types::StreamKind;
use std::cell::RefCell;
use std::time::Duration;

/// Decode `path`'s best video stream end-to-end and count the frames, mirroring the read-path
/// pattern in `tests/probe_decode.rs`. Used to compare source vs. transcoded frame counts.
fn decoded_video_frames(path: &str) -> u64 {
    let mut reader = MediaReader::open(path).unwrap();
    let vidx = reader.best_stream(StreamKind::Video).unwrap();
    let mut decoder = reader.stream(vidx).decoder().unwrap();
    let mut count = 0u64;

    for packet in reader.packets() {
        let packet = packet.unwrap();
        if packet.stream_index() != vidx {
            continue;
        }

        for frame in decoder.decode(&packet).unwrap() {
            frame.unwrap();
            count += 1;
        }
    }

    for frame in decoder.flush().unwrap() {
        frame.unwrap();
        count += 1;
    }

    count
}

/// Count decoded video frames whose best-effort timestamp falls within `[start, end]` seconds —
/// the exact gate (`best_effort` → `pts` → 0, inclusive) the transcoder applies for `trim`. Lets
/// a trim test assert the output holds precisely the source frames in the window, independent of
/// how the transcoder reaches them (linear decode today, keyframe-seek later).
fn video_frames_in_window(path: &str, start: f64, end: f64) -> u64 {
    let mut reader = MediaReader::open(path).unwrap();
    let vidx = reader.best_stream(StreamKind::Video).unwrap();
    let tb = reader.stream_time_base(vidx).unwrap().as_f64();
    let mut decoder = reader.stream(vidx).decoder().unwrap();
    let mut count = 0u64;

    let mut tally = |frame: &Frame| {
        let ts = frame.best_effort_timestamp().or_else(|| frame.pts()).unwrap_or(0);
        let secs = ts as f64 * tb;
        if secs >= start && secs <= end {
            count += 1;
        }
    };

    for packet in reader.packets() {
        let packet = packet.unwrap();

        if packet.stream_index() != vidx {
            continue;
        }

        for frame in decoder.decode(&packet).unwrap() {
            tally(&frame.unwrap());
        }
    }

    for frame in decoder.flush().unwrap() {
        tally(&frame.unwrap());
    }

    count
}

#[test]
fn one_liner_transcode_to_mp4() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();
    let out = common::temp("media_rs_oneliner.mp4");
    let _ = std::fs::remove_file(&out);

    transcode(&input).to(&out).run().unwrap();

    let info = probe(&out).unwrap();
    assert!(info.video().is_some(), "no video stream in output");
    assert!(info.duration().as_secs_f64() > 0.0);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn drop_audio_yields_no_audio_stream() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();
    let out = common::temp("media_rs_noaudio.mp4");
    let _ = std::fs::remove_file(&out);

    transcode(&input).to(&out).drop_audio().run().unwrap();

    let info = probe(&out).unwrap();
    assert!(info.video().is_some());
    assert!(info.audio().is_none(), "audio stream was not dropped");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn audio_is_stream_copied_into_the_output() {
    // Uses the one sample that carries an audio track; skips gracefully otherwise.
    let Some(input) = common::audio_sample() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();
    // Sanity-check the fixture actually has audio, else the test proves nothing.
    if probe(&input).unwrap().audio().is_none() {
        return;
    }
    let out = common::temp("media_rs_audiocopy.mp4");
    let _ = std::fs::remove_file(&out);

    // Default transcode re-encodes video and stream-copies audio.
    transcode(&input).to(&out).run().unwrap();

    let info = probe(&out).unwrap();
    assert!(info.video().is_some(), "video missing from output");
    assert!(info.audio().is_some(), "audio was not stream-copied into the output");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn drop_video_yields_audio_only_output() {
    let Some(input) = common::audio_sample() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();
    if probe(&input).unwrap().audio().is_none() {
        return;
    }
    let out = common::temp("media_rs_audioonly.m4a");
    let _ = std::fs::remove_file(&out);

    transcode(&input).to(&out).drop_video().run().unwrap();

    let info = probe(&out).unwrap();
    assert!(info.audio().is_some(), "audio missing from audio-only output");
    assert!(info.video().is_none(), "video stream was not dropped");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn builder_scale_changes_resolution() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();
    let out = common::temp("media_rs_scaled.mp4");
    let _ = std::fs::remove_file(&out);

    let job = Transcoder::builder()
        .input(&input)
        .output(&out)
        .video(
            VideoConfig::builder()
                .codec(VideoCodec::H264)
                .resolution(320, 240)
                .preset(H264Preset::Ultrafast)
                .build()
                .unwrap(),
        )
        .drop_audio()
        .build()
        .unwrap();

    let summary = job
        .run_with_progress(|p| {
            assert!(p.percent() >= 0.0 && p.percent() <= 100.0);
        })
        .unwrap();

    assert!(summary.frames > 0);

    let info = probe(&out).unwrap();
    let v = info.video().unwrap();
    assert_eq!((v.width, v.height), (320, 240));
    let _ = std::fs::remove_file(&out);
}

#[test]
fn trim_shortens_duration() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        return;
    };
    let input_path = input.to_str().unwrap().to_owned();
    let full = probe(&input_path).unwrap().duration().as_secs_f64();

    if full < 3.0 {
        return; // too short to trim meaningfully
    }

    let out = common::temp("media_rs_trim.mp4");
    let _ = std::fs::remove_file(&out);

    transcode(&input_path)
        .to(&out)
        .drop_audio()
        .trim(Duration::from_secs(1)..=Duration::from_secs(2))
        .run()
        .unwrap();

    let trimmed = probe(&out).unwrap().duration().as_secs_f64();
    assert!(trimmed < full, "trimmed duration {trimmed} not shorter than {full}");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn raw_filter_applies() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();
    let out = common::temp("media_rs_filter.mp4");
    let _ = std::fs::remove_file(&out);

    let job = Transcoder::builder()
        .input(&input)
        .output(&out)
        .drop_audio()
        .video_filter(VideoFilterChain::new().scale(640, 360))
        .build()
        .unwrap();

    job.run().unwrap();

    let v = probe(&out).unwrap().video().cloned().unwrap();
    assert_eq!((v.width, v.height), (640, 360));
    let _ = std::fs::remove_file(&out);
}

// --- characterization tests: pin behavior the pipeline refactor must preserve ---------------

#[test]
fn full_transcode_preserves_frame_count_and_duration() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();
    let in_count = decoded_video_frames(&input);
    let in_dur = probe(&input).unwrap().duration().as_secs_f64();

    let out = common::temp("media_rs_preserve.mp4");
    let _ = std::fs::remove_file(&out);

    // No filter and no trim → one encoded frame per decoded frame.
    let summary = transcode(&input).to(&out).drop_audio().run().unwrap();

    // The reported count tracks the source (small slack for start-offset boundary frames).
    assert!(
        (summary.frames as i64 - in_count as i64).abs() <= 2,
        "summary frames {} drifted from source {in_count}",
        summary.frames
    );

    // The muxed output, re-decoded, matches what the encoder reported.
    let out_count = decoded_video_frames(&out);
    assert!(
        (out_count as i64 - summary.frames as i64).abs() <= 1,
        "output frames {out_count} != summary frames {}",
        summary.frames
    );

    // Duration is preserved (catches a truncated tail / mis-flush).
    let out_dur = probe(&out).unwrap().duration().as_secs_f64();
    let tol = (in_dur * 0.10).max(0.3);
    assert!((out_dur - in_dur).abs() <= tol, "output duration {out_dur} not within {tol} of source {in_dur}");

    let _ = std::fs::remove_file(&out);
}

#[test]
fn audio_and_video_muxed_with_matching_duration() {
    let Some(input) = common::audio_sample() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();
    let info_in = probe(&input).unwrap();

    if info_in.audio().is_none() {
        return;
    }

    let in_dur = info_in.duration().as_secs_f64();
    let out = common::temp("media_rs_av_mux.mp4");
    let _ = std::fs::remove_file(&out);

    // Re-encode video, stream-copy audio, mux both.
    transcode(&input).to(&out).run().unwrap();

    let info = probe(&out).unwrap();
    assert!(info.video().is_some(), "video missing from muxed output");
    assert!(info.audio().is_some(), "audio missing from muxed output");
    let out_dur = info.duration().as_secs_f64();
    let tol = (in_dur * 0.10).max(0.3);

    assert!(
        (out_dur - in_dur).abs() <= tol,
        "muxed output duration {out_dur} not within {tol} of source {in_dur}"
    );

    let _ = std::fs::remove_file(&out);
}

#[test]
fn trim_with_audio_keeps_both_streams() {
    let Some(input) = common::audio_sample() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();
    let info_in = probe(&input).unwrap();
    if info_in.audio().is_none() || info_in.duration().as_secs_f64() < 3.0 {
        return;
    }

    let out = common::temp("media_rs_trim_av.mp4");
    let _ = std::fs::remove_file(&out);

    // Trim ~1s, keeping BOTH streams (exercises video + audio gating together).
    transcode(&input).to(&out).trim(Duration::from_secs(1)..=Duration::from_secs(2)).run().unwrap();

    let info = probe(&out).unwrap();
    assert!(info.video().is_some(), "video missing after trim");
    assert!(info.audio().is_some(), "audio missing after trim");

    // ~1s window; generous slack for container rounding / keyframe placement.
    let dur = info.duration().as_secs_f64();
    assert!(dur > 0.2 && dur < 2.5, "trimmed duration {dur} outside expected ~1s window");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn trim_frame_count_matches_source_window() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        return;
    };

    let input = input.to_str().unwrap().to_owned();
    if probe(&input).unwrap().duration().as_secs_f64() < 3.0 {
        return;
    }

    let (a, b) = (1.0_f64, 2.0_f64);
    let expected = video_frames_in_window(&input, a, b);
    if expected == 0 {
        return; // degenerate fixture: nothing in the window
    }

    let out = common::temp("media_rs_trim_count.mp4");
    let _ = std::fs::remove_file(&out);
    let summary = transcode(&input)
        .to(&out)
        .drop_audio()
        .trim(Duration::from_secs_f64(a)..=Duration::from_secs_f64(b))
        .run()
        .unwrap();

    // The output must hold exactly the source frames in the window — the invariant a keyframe
    // seek must preserve.
    assert_eq!(summary.frames, expected, "trimmed frame count drifted from the source window");

    let out_count = decoded_video_frames(&out);
    assert!(
        (out_count as i64 - expected as i64).abs() <= 1,
        "output frames {out_count} not within 1 of expected {expected}"
    );

    let _ = std::fs::remove_file(&out);
}

#[test]
fn trim_midfile_window_frame_count_matches_source() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();
    let full = probe(&input).unwrap().duration().as_secs_f64();

    if full < 6.0 {
        return;
    }

    // A 1s window near the middle — far from t=0, so a fast-trim seek must jump into the file
    // (not just start decoding from the front).
    let a = (full * 0.5).floor();
    let b = a + 1.0;
    let expected = video_frames_in_window(&input, a, b);
    if expected == 0 {
        return;
    }

    let out = common::temp("media_rs_trim_mid.mp4");
    let _ = std::fs::remove_file(&out);
    let summary = transcode(&input)
        .to(&out)
        .drop_audio()
        .trim(Duration::from_secs_f64(a)..=Duration::from_secs_f64(b))
        .run()
        .unwrap();

    assert_eq!(summary.frames, expected, "mid-file trimmed frame count drifted from the source window");

    let out_count = decoded_video_frames(&out);
    assert!(
        (out_count as i64 - expected as i64).abs() <= 1,
        "output frames {out_count} not within 1 of expected {expected}"
    );

    let _ = std::fs::remove_file(&out);
}

#[test]
fn progress_is_monotonic_and_completes() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();
    let out = common::temp("media_rs_progress.mp4");
    let _ = std::fs::remove_file(&out);

    let snapshots = RefCell::new(Vec::new());
    let summary = transcode(&input)
        .to(&out)
        .drop_audio()
        .run_with_progress(|p| snapshots.borrow_mut().push((p.frames(), p.percent())))
        .unwrap();

    let snaps = snapshots.into_inner();
    assert!(!snaps.is_empty(), "no progress callbacks fired");

    let mut last = 0u64;
    for (frames, percent) in &snaps {
        assert!(*frames >= last, "progress frames went backwards: {last} -> {frames}");
        last = *frames;
        assert!((0.0..=100.0).contains(percent), "percent out of bounds: {percent}");
    }

    // Progress never over-reports; flushed tail frames may push the final count higher.
    assert!(
        snaps.last().unwrap().0 <= summary.frames,
        "progress reported more frames ({}) than the summary ({})",
        snaps.last().unwrap().0,
        summary.frames
    );

    assert!(summary.frames > 0, "transcode produced no frames");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn typed_filters_apply_end_to_end() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();
    let out = common::temp("media_rs_typed_filters.mp4");
    let _ = std::fs::remove_file(&out);

    // Exercise the non-scale filter path through the real filter graph: denoise + colour
    // correction keep the frame geometry but run frames through `hqdn3d` and `eq`.
    let in_video = probe(&input).unwrap().video().cloned().unwrap();
    let filter = VideoFilterChain::new()
        .denoise(DenoiseLevel::Light)
        .color_correct(|c| c.brightness(0.05).saturation(1.1));
    let job = Transcoder::builder()
        .input(&input)
        .output(&out)
        .drop_audio()
        .video_filter(filter)
        .build()
        .unwrap();

    let summary = job.run().unwrap();
    assert!(summary.frames > 0);

    let v = probe(&out).unwrap().video().cloned().unwrap();
    assert_eq!((v.width, v.height), (in_video.width, in_video.height));
    let _ = std::fs::remove_file(&out);
}
