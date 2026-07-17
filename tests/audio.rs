//! Audio encoding tests: re-encode, format conversion, filters, the copy-vs-encode decision,
//! and the audio-only-container guard. Each skips gracefully when `assets/` is absent.

mod common;

use media::prelude::*;

/// Re-open `path` and return the first audio stream's codec + (sample_rate, channels).
fn audio_info(path: &str) -> Option<(AudioCodec, u32)> {
    let info = probe(path).unwrap();
    let a = info.audio()?;
    Some((a.audio_codec?, a.sample_rate as u32))
}

#[test]
fn audio_only_format_conversion_mp3_to_flac() {
    let Some(input) = common::audio_only_sample() else { return };
    let input = input.to_str().unwrap().to_owned();
    let out = common::temp("media_rs_conv.flac");
    let _ = std::fs::remove_file(&out);

    // No audio config: the .flac container's codec is incompatible with the MP3 source, so the
    // pipeline auto-encodes to FLAC rather than failing in the muxer.
    transcode(&input).to(&out).run().unwrap();

    let info = probe(&out).unwrap();
    assert!(info.video().is_none(), "audio-only input should not produce video");
    assert_eq!(audio_info(&out).map(|(c, _)| c), Some(AudioCodec::Flac));
    let _ = std::fs::remove_file(&out);
}

#[test]
fn audio_reencode_with_explicit_config() {
    let Some(input) = common::audio_only_sample() else { return };
    let input = input.to_str().unwrap().to_owned();
    let out = common::temp("media_rs_reenc.m4a");
    let _ = std::fs::remove_file(&out);

    let cfg = AudioConfig::builder()
        .codec(AudioCodec::Aac)
        .bitrate(Bitrate::kbps(96))
        .sample_rate(SampleRate::Hz44100)
        .build()
        .unwrap();
    transcode(&input).to(&out).audio(cfg).run().unwrap();

    assert_eq!(audio_info(&out), Some((AudioCodec::Aac, 44_100)));
    let _ = std::fs::remove_file(&out);
}

#[test]
fn audio_filter_forces_reencode() {
    let Some(input) = common::audio_only_sample() else { return };
    let input = input.to_str().unwrap().to_owned();
    let out = common::temp("media_rs_filtered.m4a");
    let _ = std::fs::remove_file(&out);

    let chain = AudioFilterChain::new().highpass(80.0).volume(Decibels(2.0));
    transcode(&input).to(&out).audio_filter(chain).run().unwrap();

    assert!(audio_info(&out).is_some(), "filtered audio produced no audio stream");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn video_into_audio_only_container_fails_without_drop_video() {
    // video2.mp4 has a video stream; targeting .mp3 must fail loudly unless video is dropped.
    let Some(input) = common::audio_sample() else { return };
    let input = input.to_str().unwrap().to_owned();
    if probe(&input).unwrap().video().is_none() {
        return;
    }
    let out = common::temp("media_rs_guard.mp3");
    let _ = std::fs::remove_file(&out);

    let err = transcode(&input).to(&out).run();
    assert!(matches!(err, Err(Error::InvalidConfig(_))), "expected a loud error, got {err:?}");

    // With .drop_video() the same conversion succeeds, producing an MP3 audio stream.
    transcode(&input).to(&out).drop_video().run().unwrap();
    assert_eq!(audio_info(&out).map(|(c, _)| c), Some(AudioCodec::Mp3));
    let _ = std::fs::remove_file(&out);
}

#[test]
fn audio_stream_copy_is_the_default_when_compatible() {
    // video2.mp4's audio (AAC) into an .mp4/.mkv is copy-compatible, so no re-encode happens
    // and the codec is preserved verbatim.
    let Some(input) = common::audio_sample() else { return };
    let input = input.to_str().unwrap().to_owned();
    let Some((src_codec, _)) = audio_info(&input) else { return };
    let out = common::temp("media_rs_copy.mkv");
    let _ = std::fs::remove_file(&out);

    transcode(&input).to(&out).run().unwrap();

    assert_eq!(audio_info(&out).map(|(c, _)| c), Some(src_codec), "audio should be copied unchanged");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn tier3_audio_encoder_roundtrip() {
    let Some(input) = common::audio_only_sample() else { return };
    let input = input.to_str().unwrap().to_owned();
    let out = common::temp("media_rs_tier3.m4a");
    let _ = std::fs::remove_file(&out);

    let mut reader = MediaReader::open(&input).unwrap();
    let aidx = reader.best_stream(StreamKind::Audio).unwrap();
    let mut decoder = reader.stream(aidx).decoder().unwrap();
    let mut encoder = AudioEncoder::builder().codec(AudioCodec::Aac).from_decoder(&decoder).build().unwrap();

    let mut writer = MediaWriter::create(&out).unwrap();
    let out_idx = writer.add_stream_from_encoder(&encoder).unwrap();
    writer.write_header().unwrap();

    for packet in reader.packets() {
        let packet = packet.unwrap();
        if packet.stream_index() != aidx {
            continue;
        }
        for frame in decoder.decode(&packet).unwrap() {
            for mut pkt in encoder.encode(&frame.unwrap()).unwrap() {
                pkt.set_stream_index(out_idx);
                writer.write_packet(&mut pkt).unwrap();
            }
        }
    }
    for mut pkt in encoder.flush().unwrap() {
        pkt.set_stream_index(out_idx);
        writer.write_packet(&mut pkt).unwrap();
    }
    writer.write_trailer().unwrap();

    assert_eq!(audio_info(&out).map(|(c, _)| c), Some(AudioCodec::Aac));
    let _ = std::fs::remove_file(&out);
}
