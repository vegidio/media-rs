//! Tier 1 (one-liner) and Tier 2 (builder) transcode tests, verified by re-probing output.

mod common;

use media::prelude::*;

fn temp(name: &str) -> String {
    std::env::temp_dir()
        .join(name)
        .to_str()
        .unwrap()
        .to_owned()
}

#[test]
fn one_liner_transcode_to_mp4() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();
    let out = temp("media_rs_oneliner.mp4");
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
    let out = temp("media_rs_noaudio.mp4");
    let _ = std::fs::remove_file(&out);

    transcode(&input).to(&out).drop_audio().run().unwrap();

    let info = probe(&out).unwrap();
    assert!(info.video().is_some());
    assert!(info.audio().is_none(), "audio stream was not dropped");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn builder_scale_changes_resolution() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();
    let out = temp("media_rs_scaled.mp4");
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
    let summary = job.run_with_progress(|p| {
        assert!(p.percent() >= 0.0 && p.percent() <= 100.0);
    }).unwrap();
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
    let out = temp("media_rs_trim.mp4");
    let _ = std::fs::remove_file(&out);

    transcode(&input_path)
        .to(&out)
        .drop_audio()
        .trim(1.0..=2.0)
        .run()
        .unwrap();

    let trimmed = probe(&out).unwrap().duration().as_secs_f64();
    assert!(
        trimmed < full,
        "trimmed duration {trimmed} not shorter than {full}"
    );
    let _ = std::fs::remove_file(&out);
}

#[test]
fn raw_filter_applies() {
    let Some(input) = common::sample_videos().into_iter().next() else {
        return;
    };
    let input = input.to_str().unwrap().to_owned();
    let out = temp("media_rs_filter.mp4");
    let _ = std::fs::remove_file(&out);

    let job = Transcoder::builder()
        .input(&input)
        .output(&out)
        .drop_audio()
        .video_filter(FilterChain::new().scale(640, 360))
        .build()
        .unwrap();
    job.run().unwrap();

    let v = probe(&out).unwrap().video().cloned().unwrap();
    assert_eq!((v.width, v.height), (640, 360));
    let _ = std::fs::remove_file(&out);
}
