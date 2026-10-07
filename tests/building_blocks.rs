//! The low-level building blocks a custom pipeline composes: a public `VideoFilter`, a decoder's sample aspect ratio,
//! a stream's rotation, packets kept as bytes and rebuilt, and encoder options.

mod common;

use media::codec::VideoEncoder;
use media::prelude::*;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

/// A writer whose bytes the test can read back while the `MediaWriter` owns a clone of it.
#[derive(Clone, Default)]
struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl SharedBuf {
    fn bytes(&self) -> Vec<u8> {
        self.0.lock().unwrap().clone()
    }
}

impl Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The first `count` decoded frames of `path`'s video stream, with the decoder and the stream's time base.
fn first_frames(path: &str, count: usize) -> (Decoder, Rational, Vec<Frame>) {
    let mut reader = MediaReader::open(path).unwrap();
    let video = reader.best_stream(StreamKind::Video).unwrap();
    let time_base = reader.stream_time_base(video).unwrap();
    let mut decoder = reader.stream(video).decoder().unwrap();
    let mut frames = Vec::new();
    for packet in reader.packets() {
        let packet = packet.unwrap();
        if packet.stream_index() != video {
            continue;
        }
        frames.extend(decoder.decode(&packet).unwrap().map(Result::unwrap));
        if frames.len() >= count {
            break;
        }
    }
    frames.truncate(count);
    (decoder, time_base, frames)
}

#[test]
fn a_video_filter_scales_a_decoders_frames() {
    let Some(input) = common::audio_sample() else { return };
    let (decoder, time_base, frames) = first_frames(input.to_str().unwrap(), 5);

    let chain = VideoFilterChain::raw("scale=320:180,format=yuv420p");
    let mut filter = VideoFilter::new(&decoder, time_base, &chain).unwrap();

    assert_eq!((filter.output_width(), filter.output_height()), (320, 180));
    assert_eq!(filter.output_pixel_format(), PixelFormat::Yuv420p);
    assert_eq!(filter.output_time_base(), time_base);
    let mut out = Vec::new();
    for frame in frames {
        out.extend(filter.filter(frame).unwrap());
    }
    out.extend(filter.flush().unwrap());
    assert_eq!(out.len(), 5);
    for frame in &out {
        assert_eq!((frame.width(), frame.height(), frame.pixel_format()), (320, 180, PixelFormat::Yuv420p));
    }
}

#[test]
fn an_empty_chain_passes_frames_through() {
    let Some(input) = common::audio_sample() else { return };
    let (decoder, time_base, frames) = first_frames(input.to_str().unwrap(), 2);
    let mut filter = VideoFilter::new(&decoder, time_base, &VideoFilterChain::new()).unwrap();

    assert_eq!((filter.output_width(), filter.output_height()), (decoder.width(), decoder.height()));
    let pts: Vec<_> = frames.iter().map(Frame::pts).collect();
    let out: Vec<_> = frames.into_iter().flat_map(|frame| filter.filter(frame).unwrap()).collect();
    assert_eq!(out.iter().map(Frame::pts).collect::<Vec<_>>(), pts);
}

#[test]
fn a_square_pixel_source_reports_one_to_one() {
    let Some(input) = common::audio_sample() else { return };
    let mut reader = MediaReader::open(input.to_str().unwrap()).unwrap();
    let video = reader.best_stream(StreamKind::Video).unwrap();

    assert_eq!(reader.stream(video).decoder().unwrap().sample_aspect_ratio(), Rational::ONE);
}

// --- rotation -------------------------------------------------------------------------------------------------------

/// Copies `input`'s streams into a plain MP4 at `output`.
fn copy_to_mp4(input: &str, output: &str) {
    let mut reader = MediaReader::open(input).unwrap();
    let mut writer = MediaWriter::create(output).unwrap();
    let outputs: Vec<_> = (0..reader.stream_count()).map(|i| writer.add_stream_copy(&reader, i).unwrap()).collect();
    writer.write_header().unwrap();
    for packet in reader.packets() {
        let mut packet = packet.unwrap();
        packet.set_stream_index(outputs[packet.stream_index()]);
        writer.write_packet(&mut packet).unwrap();
    }
    writer.write_trailer().unwrap();
}

/// Writes the display matrix for a turn of `clockwise` degrees into every `tkhd` box of the MP4 at `path`, the way a
/// phone stores a portrait recording.
fn rotate_tracks(path: &str, clockwise: f64) {
    let mut matrix = [0i32; 9];
    // SAFETY: av_display_rotation_set writes nine i32s into the array. Unlike av_display_rotation_get, it takes a
    // clockwise angle.
    unsafe { media::sys::av_display_rotation_set(matrix.as_mut_ptr(), clockwise) };

    let mut bytes = std::fs::read(path).unwrap();
    let mut patched = 0;
    for at in (4..bytes.len() - 4).filter(|&at| &bytes[at..at + 4] == b"tkhd").collect::<Vec<_>>() {
        // After the header: version and flags, then times, ids and duration (20 or 32 bytes), then 16 bytes of
        // reserved, layer, group and volume fields, then the matrix.
        let body = at + 4;
        let start = body + 4 + if bytes[body] == 1 { 32 } else { 20 } + 16;
        for (i, value) in matrix.iter().enumerate() {
            bytes[start + i * 4..start + i * 4 + 4].copy_from_slice(&value.to_be_bytes());
        }
        patched += 1;
    }
    assert!(patched > 0, "no tkhd box");
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn a_rotated_stream_reports_its_clockwise_turn() {
    let Some(input) = common::sample_videos().into_iter().next() else { return };
    for clockwise in [90, 180, 270, 0] {
        let output = common::temp(&format!("media_rs_rotated_{clockwise}.mp4"));
        copy_to_mp4(input.to_str().unwrap(), &output);
        rotate_tracks(&output, f64::from(clockwise));

        let reader = MediaReader::open(&output).unwrap();
        let video = reader.best_stream(StreamKind::Video).unwrap();
        // An identity matrix is no rotation at all, which the demuxer doesn't report.
        let expected = (clockwise != 0).then_some(clockwise);
        assert_eq!(reader.stream_rotation(video).unwrap(), expected, "{clockwise}°");
        std::fs::remove_file(output).ok();
    }
}

#[test]
fn an_unrotated_stream_reports_none() {
    let Some(input) = common::audio_sample() else { return };
    let reader = MediaReader::open(input.to_str().unwrap()).unwrap();

    for index in 0..reader.stream_count() {
        assert_eq!(reader.stream_rotation(index).unwrap(), None);
    }
    assert!(matches!(reader.stream_rotation(99), Err(Error::StreamOutOfRange(99))));
}

// --- packets --------------------------------------------------------------------------------------------------------

/// `input` remuxed into fragmented MP4 in memory, each packet passed through `rebuild` first.
fn remux_fmp4(input: &str, rebuild: impl Fn(Packet) -> Packet) -> Vec<u8> {
    let sink = SharedBuf::default();
    let mut reader = MediaReader::open(input).unwrap();
    let mut writer = MediaWriter::builder().writer(sink.clone()).fragmented_mp4().build().unwrap();
    let outputs: Vec<_> = (0..reader.stream_count()).map(|i| writer.add_stream_copy(&reader, i).unwrap()).collect();
    writer.write_header().unwrap();
    for packet in reader.packets() {
        let packet = packet.unwrap();
        let index = packet.stream_index();
        let mut packet = rebuild(packet);
        packet.set_stream_index(outputs[index]);
        writer.write_packet(&mut packet).unwrap();
    }
    writer.write_trailer().unwrap();
    drop(writer);
    sink.bytes()
}

#[test]
fn a_packet_rebuilt_from_its_parts_writes_the_same_bytes() {
    let Some(input) = common::audio_sample() else { return };
    let input = input.to_str().unwrap();

    let original = remux_fmp4(input, |packet| packet);
    let rebuilt = remux_fmp4(input, |packet| {
        let data = packet.data().to_vec();
        Packet::from_data(&data, packet.pts(), packet.dts(), packet.duration(), packet.is_keyframe()).unwrap()
    });

    assert!(!original.is_empty());
    assert!(original == rebuilt, "the rebuilt packets wrote different bytes");
}

#[test]
fn a_packet_from_data_keeps_what_it_was_given() {
    let packet = Packet::from_data(&[1, 2, 3, 4], 900, 600, 300, true).unwrap();

    assert_eq!(packet.data(), [1, 2, 3, 4]);
    assert_eq!((packet.pts(), packet.dts(), packet.duration(), packet.is_keyframe()), (900, 600, 300, true));
    assert_eq!(packet.stream_index(), 0);
    assert!(Packet::from_data(&[], 0, 0, 0, false).unwrap().data().is_empty());
}

// --- encoder options ------------------------------------------------------------------------------------------------

/// Every packet libx264 makes of `frames`, built with `options`.
fn encode(decoder: &Decoder, time_base: Rational, frames: &[Frame], options: &[(&str, &str)]) -> Vec<Packet> {
    let mut builder = VideoEncoder::builder()
        .codec(VideoCodec::H264)
        .from_decoder(decoder)
        .time_base(time_base)
        .preset(H264Preset::Ultrafast);
    for (key, value) in options {
        builder = builder.option(key, value);
    }
    let mut encoder = builder.build().unwrap();

    let mut packets = Vec::new();
    for frame in frames {
        packets.extend(encoder.encode(frame).unwrap().map(Result::unwrap));
    }
    packets.extend(encoder.flush().unwrap().map(Result::unwrap));
    packets
}

#[test]
fn the_bf_option_turns_b_frames_off() {
    let Some(input) = common::audio_sample() else { return };
    let (decoder, time_base, mut frames) = first_frames(input.to_str().unwrap(), 48);
    for frame in &mut frames {
        let pts = frame.best_effort_timestamp().unwrap();
        frame.set_pts(pts);
    }
    let reordered = |packets: &[Packet]| packets.iter().any(|packet| packet.pts() != packet.dts());

    // Ultrafast has no B-frames of its own, so the default is set back to x264's usual 3 to show the option at work.
    assert!(reordered(&encode(&decoder, time_base, &frames, &[("bf", "3")])), "b-frames were expected");
    let packets = encode(&decoder, time_base, &frames, &[("bf", "3"), ("bf", "0")]);
    assert_eq!(packets.len(), 48);
    assert!(!reordered(&packets), "a packet was reordered, so there are b-frames");
}

#[test]
fn an_unknown_encoder_option_fails_build_naming_it() {
    let result = VideoEncoder::builder()
        .codec(VideoCodec::H264)
        .resolution(64, 64)
        .option("nonsense", "1")
        .build();

    match result {
        Err(Error::UnknownOption(names)) => assert_eq!(names, "nonsense"),
        Err(other) => panic!("expected UnknownOption, got {other}"),
        Ok(_) => panic!("an unknown option was accepted"),
    }
}

#[test]
fn generic_and_private_options_are_both_accepted() {
    let encoder = VideoEncoder::builder()
        .codec(VideoCodec::H264)
        .resolution(64, 64)
        .option("sc_threshold", "0")
        .option("tune", "zerolatency")
        .build();

    assert!(encoder.is_ok(), "{:?}", encoder.err());
}

#[test]
fn a_decoded_keyframe_does_not_force_one_on_the_encoder() {
    // `video2.mp4` has a keyframe at 10.4 s, frame 250.
    let Some(input) = common::audio_sample() else { return };
    let (decoder, time_base, mut frames) = first_frames(input.to_str().unwrap(), 300);
    for frame in &mut frames {
        let pts = frame.best_effort_timestamp().unwrap();
        frame.set_pts(pts);
    }

    let packets = encode(&decoder, time_base, &frames, &[("g", "1000"), ("sc_threshold", "0")]);

    assert_eq!(packets.len(), 300);
    assert_eq!(packets.iter().filter(|packet| packet.is_keyframe()).count(), 1);
    assert!(packets[0].is_keyframe());
}

#[test]
fn an_audio_filter_cuts_at_an_exact_sample() {
    let Some(input) = common::audio_sample() else { return };
    let mut reader = MediaReader::open(input.to_str().unwrap()).unwrap();
    let audio = reader.best_stream(StreamKind::Audio).unwrap();
    let time_base = reader.stream_time_base(audio).unwrap();
    let mut decoder = reader.stream(audio).decoder().unwrap();
    let rate = decoder.sample_rate();
    // 1.01 s is inside a decoded frame, which holds 1024 samples.
    let mut filter = AudioFilter::new(&decoder, time_base, &AudioFilterChain::raw("atrim=start=1.01")).unwrap();

    let (mut total, mut kept) = (0u64, Vec::new());
    for packet in reader.packets() {
        let packet = packet.unwrap();
        if packet.stream_index() != audio {
            continue;
        }
        let frames: Vec<Frame> = decoder.decode(&packet).unwrap().map(Result::unwrap).collect();
        for mut frame in frames {
            total += u64::from(frame.sample_count());
            let pts = frame.best_effort_timestamp().unwrap();
            frame.set_pts(pts);
            kept.extend(filter.filter(frame).unwrap());
        }
    }
    kept.extend(filter.flush().unwrap());

    let first = kept[0].pts().unwrap() as f64 * time_base.as_f64();
    assert!((first - 1.01).abs() < 1.0 / f64::from(rate), "the first kept sample is at {first} s");
    let skipped = total - kept.iter().map(|frame| u64::from(frame.sample_count())).sum::<u64>();
    assert_eq!(skipped, (1.01 * f64::from(rate)).round() as u64);
}
