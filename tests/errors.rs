//! Error-path tests. These cover the failure scenarios the rest of the suite never hits:
//! bad inputs, missing streams, and misused builders. Several need no assets at all.

mod common;

use media::prelude::*;
use media::types::StreamKind;

#[test]
fn open_missing_input_errors() {
    match MediaReader::open("/no/such/file_media_rs.mp4") {
        Err(Error::OpenInput(_)) => {}
        Err(e) => panic!("expected OpenInput, got {e:?}"),
        Ok(_) => panic!("opening a missing file unexpectedly succeeded"),
    }
}

#[test]
fn probe_missing_input_errors() {
    assert!(probe("/no/such/file_media_rs.mp4").is_err());
}

#[test]
fn video_config_requires_a_codec() {
    let err = VideoConfig::builder().build().unwrap_err();
    assert!(matches!(err, Error::InvalidConfig(_)), "expected InvalidConfig, got {err:?}");
}

#[test]
fn transcoder_requires_input_and_output() {
    // Missing input.
    match Transcoder::builder().output("out.mp4").build() {
        Err(Error::InvalidConfig(_)) => {}
        other => panic!("expected InvalidConfig for missing input, got {:?}", other.err()),
    }

    // Missing output.
    match Transcoder::builder().input("in.mp4").build() {
        Err(Error::InvalidConfig(_)) => {}
        other => panic!("expected InvalidConfig for missing output, got {:?}", other.err()),
    }
}

#[test]
fn best_stream_for_absent_audio_errors() {
    // video1.mp4 is video-only, so asking for its best audio stream must fail cleanly.
    let video_only = common::asset("video1.mp4");
    if !video_only.exists() {
        return;
    }
    let reader = MediaReader::open(video_only.to_str().unwrap()).unwrap();
    let err = reader.best_stream(StreamKind::Audio).unwrap_err();
    assert!(matches!(err, Error::NoAudioStream), "expected NoAudioStream, got {err:?}");
}

#[test]
fn dropping_every_stream_errors() {
    // Dropping video from a video-only file leaves nothing to transcode, which the pipeline
    // must reject rather than producing an empty file.
    let video_only = common::asset("video1.mp4");
    if !video_only.exists() {
        return;
    }
    let out = common::temp("media_rs_empty.mp4");
    let _ = std::fs::remove_file(&out);

    let err = transcode(video_only.to_str().unwrap()).to(&out).drop_video().run().unwrap_err();
    assert!(matches!(err, Error::InvalidConfig(_)), "expected InvalidConfig, got {err:?}");
    let _ = std::fs::remove_file(&out);
}
