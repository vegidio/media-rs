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

#[test]
fn probe_reports_the_container_and_codec_names() {
    for path in common::sample_videos() {
        let p = path.to_str().unwrap();
        let info = probe(p).unwrap();
        assert_eq!(info.format_name(), "mov,mp4,m4a,3gp,3g2,mj2", "{p}");
        assert_eq!(info.video().unwrap().codec_name, "h264", "{p}");
    }

    if let Some(path) = common::audio_sample() {
        let info = probe(path.to_str().unwrap()).unwrap();
        assert_eq!(info.audio().unwrap().codec_name, "aac");
    }
}

#[test]
fn probe_reports_matroska_for_a_remux_to_mkv() {
    let Some(path) = common::sample_videos().into_iter().next() else { return };
    let out = common::temp("media_rs_probe_names.mkv");

    let mut reader = MediaReader::open(path.to_str().unwrap()).unwrap();
    let mut writer = MediaWriter::create(&out).unwrap();
    let mut out_index = Vec::new();
    for i in 0..reader.stream_count() {
        out_index.push(writer.add_stream_copy(&reader, i).unwrap());
    }
    writer.write_header().unwrap();
    for packet in reader.packets() {
        let mut packet = packet.unwrap();
        packet.set_stream_index(out_index[packet.stream_index()]);
        writer.write_packet(&mut packet).unwrap();
    }
    writer.write_trailer().unwrap();
    drop(writer);

    let info = probe(&out).unwrap();
    assert_eq!(info.format_name(), "matroska,webm");
    assert_eq!(info.video().unwrap().codec_name, "h264");
    std::fs::remove_file(&out).ok();
}

#[test]
fn probe_names_a_codec_without_a_video_codec_variant() {
    let out = common::temp("media_rs_probe_mpeg4.mkv");
    encode_mpeg4(&out);

    let info = probe(&out).unwrap();
    let video = info.video().unwrap();
    assert_eq!(video.codec_name, "mpeg4");
    assert_eq!(video.video_codec, None);
    // FFmpeg 8.1 leaves out MPEG-4 Part 2's profile and level, and says so in its source.
    assert_eq!(video.codec_string.as_deref(), Some("mp4v.20"));
    std::fs::remove_file(&out).ok();
}

#[test]
fn probe_reports_rfc_6381_codec_strings() {
    // `ffprobe -show_streams` reports every sample's video as H.264 High (profile_idc 100 = 0x64) at level 31 (0x1f),
    // so its avcC string is `avc1.64CC1f`, where CC is the constraint-flags byte.
    for path in common::sample_videos() {
        let p = path.to_str().unwrap();
        let info = probe(p).unwrap();
        let codec = info.video().unwrap().codec_string.clone().unwrap_or_else(|| panic!("{p}: no codec string"));
        assert!(codec.len() == 11 && codec.starts_with("avc1.64") && codec.ends_with("1f"), "{p}: {codec}");
    }

    // ...and its audio as AAC LC, object type 2.
    if let Some(path) = common::audio_sample() {
        let info = probe(path.to_str().unwrap()).unwrap();
        assert_eq!(info.audio().unwrap().codec_string.as_deref(), Some("mp4a.40.2"));
    }
}

/// Write one second of 64x64 MPEG-4 Part 2 to `path`, through the raw FFI: the safe encoder only offers the five
/// `VideoCodec`s, and this test needs a codec outside them.
fn encode_mpeg4(path: &str) {
    use media::sys;
    use std::ffi::CString;
    use std::ptr;

    fn ok(ret: i32) {
        assert!(ret >= 0, "ffmpeg error {ret}");
    }

    let cpath = CString::new(path).unwrap();
    // SAFETY: a straight-line use of the FFmpeg muxing/encoding API; every pointer is checked for null after
    // allocation, used only while alive, and freed once at the end.
    unsafe {
        let mut oc = ptr::null_mut();
        ok(sys::avformat_alloc_output_context2(&mut oc, ptr::null(), ptr::null(), cpath.as_ptr()));
        assert!(!oc.is_null());

        let codec = sys::avcodec_find_encoder_by_name(c"mpeg4".as_ptr());
        assert!(!codec.is_null(), "no native mpeg4 encoder");
        let mut enc = sys::avcodec_alloc_context3(codec);
        assert!(!enc.is_null());
        (*enc).width = 64;
        (*enc).height = 64;
        (*enc).pix_fmt = sys::AVPixelFormat_AV_PIX_FMT_YUV420P;
        (*enc).time_base = sys::AVRational { num: 1, den: 24 };
        (*enc).framerate = sys::AVRational { num: 24, den: 1 };
        if (*(*oc).oformat).flags & sys::AVFMT_GLOBALHEADER as i32 != 0 {
            (*enc).flags |= sys::AV_CODEC_FLAG_GLOBAL_HEADER as i32;
        }
        ok(sys::avcodec_open2(enc, codec, ptr::null_mut()));

        let st = sys::avformat_new_stream(oc, ptr::null());
        assert!(!st.is_null());
        ok(sys::avcodec_parameters_from_context((*st).codecpar, enc));
        (*st).time_base = (*enc).time_base;

        ok(sys::avio_open(&mut (*oc).pb, cpath.as_ptr(), sys::AVIO_FLAG_WRITE as i32));
        ok(sys::avformat_write_header(oc, ptr::null_mut()));

        let mut frame = sys::av_frame_alloc();
        (*frame).format = (*enc).pix_fmt;
        (*frame).width = 64;
        (*frame).height = 64;
        ok(sys::av_frame_get_buffer(frame, 0));
        let mut pkt = sys::av_packet_alloc();

        let drain = |enc: *mut sys::AVCodecContext| {
            while sys::avcodec_receive_packet(enc, pkt) >= 0 {
                sys::av_packet_rescale_ts(pkt, (*enc).time_base, (*st).time_base);
                (*pkt).stream_index = (*st).index;
                ok(sys::av_interleaved_write_frame(oc, pkt));
            }
        };
        for i in 0..24 {
            ok(sys::av_frame_make_writable(frame));
            for plane in 0..3 {
                let rows = if plane == 0 { 64 } else { 32 };
                let len = ((*frame).linesize[plane] * rows) as usize;
                ptr::write_bytes((*frame).data[plane], (i * 10) as u8, len);
            }
            (*frame).pts = i;
            ok(sys::avcodec_send_frame(enc, frame));
            drain(enc);
        }
        ok(sys::avcodec_send_frame(enc, ptr::null()));
        drain(enc);

        ok(sys::av_write_trailer(oc));
        sys::av_packet_free(&mut pkt);
        sys::av_frame_free(&mut frame);
        sys::avcodec_free_context(&mut enc);
        sys::avio_closep(&mut (*oc).pb);
        sys::avformat_free_context(oc);
    }
}
