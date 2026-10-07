//! `MediaWriter::builder()`: muxer options, writing to any `Write`, fragmented MP4 and `flush`.

mod common;

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

/// Copy every stream of `input` into `writer`, calling `before` ahead of each packet.
fn remux_with(
    input: &str,
    mut writer: MediaWriter,
    mut before: impl FnMut(&mut MediaWriter, &Packet) -> Result<()>,
) -> Result<()> {
    let mut reader = MediaReader::open(input)?;
    let mut out_index = Vec::new();
    for i in 0..reader.stream_count() {
        out_index.push(writer.add_stream_copy(&reader, i)?);
    }
    writer.write_header()?;
    for packet in reader.packets() {
        let mut packet = packet?;
        before(&mut writer, &packet)?;
        packet.set_stream_index(out_index[packet.stream_index()]);
        writer.write_packet(&mut packet)?;
    }
    writer.write_trailer()
}

fn remux(input: &str, writer: MediaWriter) -> Result<()> {
    remux_with(input, writer, |_, _| Ok(()))
}

/// The number of video frames `path` decodes to.
fn count_video_frames(path: &str) -> u64 {
    let mut reader = MediaReader::open(path).unwrap();
    let video = reader.best_stream(StreamKind::Video).unwrap();
    let mut decoder = reader.stream(video).decoder().unwrap();
    let mut count = 0;
    for packet in reader.packets() {
        let packet = packet.unwrap();
        if packet.stream_index() == video {
            count += decoder.decode(&packet).unwrap().collect::<Result<Vec<_>>>().unwrap().len() as u64;
        }
    }
    count + decoder.flush().unwrap().collect::<Result<Vec<_>>>().unwrap().len() as u64
}

// --- ISO BMFF -------------------------------------------------------------------------------------------------------

fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(b[at..at + 4].try_into().unwrap())
}

/// Split `data` into `(type, payload)` boxes, or `None` if it doesn't end on a box boundary.
fn boxes(data: &[u8]) -> Option<Vec<(String, &[u8])>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < data.len() {
        if data.len() - at < 8 {
            return None;
        }
        let kind = String::from_utf8_lossy(&data[at + 4..at + 8]).into_owned();
        let (header, size) = match be32(data, at) {
            1 => (16, u64::from_be_bytes(data.get(at + 8..at + 16)?.try_into().unwrap()) as usize),
            0 => (8, data.len() - at),
            size => (8, size as usize),
        };
        if size < header || data.len() - at < size {
            return None;
        }
        out.push((kind, &data[at + header..at + size]));
        at += size;
    }
    Some(out)
}

fn child<'a>(payload: &'a [u8], kind: &str) -> Vec<&'a [u8]> {
    boxes(payload).unwrap().into_iter().filter(|(k, _)| k == kind).map(|(_, p)| p).collect()
}

fn top_level_types(data: &[u8]) -> Vec<String> {
    boxes(data)
        .expect("the output doesn't end on a box boundary")
        .into_iter()
        .map(|(k, _)| k)
        .collect()
}

/// `ftyp`, `moov`, then `moof`/`mdat` pairs. Returns the number of pairs.
fn assert_fragmented(types: &[String]) -> usize {
    assert!(types.len() >= 2 && types[0] == "ftyp" && types[1] == "moov", "{types:?}");
    let fragments = &types[2..];
    assert!(fragments.len().is_multiple_of(2), "{types:?}");
    assert!(fragments.chunks(2).all(|pair| pair[0] == "moof" && pair[1] == "mdat"), "{types:?}");
    fragments.len() / 2
}

/// The video track's id and its `trex` default sample flags.
fn video_track(moov: &[u8]) -> (u32, u32) {
    let id = child(moov, "trak")
        .into_iter()
        .find_map(|trak| {
            let hdlr = child(child(trak, "mdia")[0], "hdlr")[0];
            (&hdlr[8..12] == b"vide").then(|| {
                let tkhd = child(trak, "tkhd")[0];
                be32(tkhd, if tkhd[0] == 1 { 20 } else { 12 })
            })
        })
        .expect("no video track");
    let trex = child(child(moov, "mvex")[0], "trex").into_iter().find(|t| be32(t, 4) == id).unwrap();
    (id, be32(trex, 20))
}

/// `true` if every `moof`'s first video sample is a sync sample (or the fragment holds no video).
fn fragments_start_on_keyframes(data: &[u8]) -> bool {
    let top = boxes(data).unwrap();
    let moov = top.iter().find(|(k, _)| k == "moov").unwrap().1;
    let (video_id, trex_flags) = video_track(moov);
    top.iter().filter(|(k, _)| k == "moof").all(|(_, moof)| {
        child(moof, "traf").into_iter().all(|traf| {
            let tfhd = child(traf, "tfhd")[0];
            if be32(tfhd, 4) != video_id {
                return true;
            }
            let tf_flags = be32(tfhd, 0) & 0xff_ffff;
            let mut at = 8;
            for (flag, len) in [(0x01, 8), (0x02, 4), (0x08, 4), (0x10, 4)] {
                if tf_flags & flag != 0 {
                    at += len;
                }
            }
            let default_flags = if tf_flags & 0x20 != 0 { be32(tfhd, at) } else { trex_flags };

            let trun = child(traf, "trun")[0];
            let tr_flags = be32(trun, 0) & 0xff_ffff;
            let mut at = 8 + if tr_flags & 0x01 != 0 { 4 } else { 0 };
            let first = if tr_flags & 0x04 != 0 {
                be32(trun, at)
            } else if tr_flags & 0x400 != 0 {
                at += [0x100, 0x200].iter().filter(|f| tr_flags & **f != 0).count() * 4;
                be32(trun, at)
            } else {
                default_flags
            };
            // sample_is_non_sync_sample
            first & 0x0001_0000 == 0
        })
    })
}

// --- 3.1: builder and options ---------------------------------------------------------------------------------------

#[test]
fn an_unknown_option_fails_write_header_naming_it() {
    let Some(input) = common::audio_sample() else { return };
    let out = common::temp("media_rs_writer_unknown.mp4");
    let writer = MediaWriter::builder().path(&out).option("no_such_option", "1").build().unwrap();

    let err = remux(input.to_str().unwrap(), writer).unwrap_err();

    assert!(matches!(&err, Error::UnknownOption(keys) if keys == "no_such_option"), "{err:?}");
    std::fs::remove_file(&out).ok();
}

#[test]
fn fragmented_mp4_keeps_other_options() {
    let Some(input) = common::audio_sample() else { return };
    let input = input.to_str().unwrap();
    let fragments = |out: &str, builder: media::format::MediaWriterBuilder| {
        remux(input, builder.path(out).build().unwrap()).unwrap();
        let count = assert_fragmented(&top_level_types(&std::fs::read(out).unwrap()));
        std::fs::remove_file(out).ok();
        count
    };

    let keyframes_only = fragments(&common::temp("media_rs_writer_frag.mp4"), MediaWriter::builder().fragmented_mp4());
    // A 0.1 s fragment duration cuts between keyframes too, so it only adds fragments if it reached the muxer.
    let with_duration = fragments(
        &common::temp("media_rs_writer_frag_duration.mp4"),
        MediaWriter::builder().option("frag_duration", "100000").fragmented_mp4(),
    );

    assert!(keyframes_only > 0);
    assert!(with_duration > keyframes_only, "{with_duration} <= {keyframes_only}");
}

#[test]
fn fragmented_mp4_keeps_its_flags_whatever_the_movflags_order() {
    let Some(input) = common::audio_sample() else { return };
    let input = input.to_str().unwrap();
    // A writer is never seeked, so without the fragmenting flags the mp4 muxer would refuse it at `write_header`.
    let fragments = |builder: media::format::MediaWriterBuilder| {
        let buf = SharedBuf::default();
        remux(input, builder.writer(buf.clone()).build().unwrap()).unwrap();
        assert_fragmented(&top_level_types(&buf.bytes()))
    };

    let before = fragments(MediaWriter::builder().option("movflags", "negative_cts_offsets").fragmented_mp4());
    let after = fragments(MediaWriter::builder().fragmented_mp4().option("movflags", "negative_cts_offsets"));

    assert!(before > 0);
    assert_eq!(before, after);
}

#[test]
fn a_builder_needs_exactly_one_of_path_or_writer() {
    let neither = MediaWriter::builder().format("mp4").build();
    let both = MediaWriter::builder().path(common::temp("media_rs_writer_both.mp4")).writer(io::sink()).build();

    assert!(matches!(neither, Err(Error::InvalidConfig(_))));
    assert!(matches!(both, Err(Error::InvalidConfig(_))));
}

// --- 3.2: writing to a `Write` --------------------------------------------------------------------------------------

#[test]
fn fragmented_mp4_streams_into_a_writer() {
    let Some(input) = common::audio_sample() else { return };
    let input = input.to_str().unwrap();
    let buf = SharedBuf::default();

    remux(input, MediaWriter::builder().writer(buf.clone()).fragmented_mp4().build().unwrap()).unwrap();

    let bytes = buf.bytes();
    assert!(assert_fragmented(&top_level_types(&bytes)) > 0);
    assert!(fragments_start_on_keyframes(&bytes));

    let out = common::temp("media_rs_writer_stream.mp4");
    std::fs::write(&out, &bytes).unwrap();
    assert_eq!(count_video_frames(&out), count_video_frames(input));
    std::fs::remove_file(&out).ok();
}

#[test]
fn a_failing_writer_surfaces_its_error() {
    /// Fails on its third write.
    struct FailThird(u32);
    impl Write for FailThird {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0 += 1;
            if self.0 == 3 { Err(io::Error::other("third write refused")) } else { Ok(buf.len()) }
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let Some(input) = common::audio_sample() else { return };
    let writer = MediaWriter::builder().writer(FailThird(0)).fragmented_mp4().build().unwrap();

    let err = remux(input.to_str().unwrap(), writer).unwrap_err();

    assert!(matches!(&err, Error::Write(e) if e.to_string() == "third write refused"), "{err:?}");
}

#[test]
fn a_panicking_writer_surfaces_as_a_write_error() {
    struct Panics;
    impl Write for Panics {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            panic!("writer exploded");
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let Some(input) = common::audio_sample() else { return };
    let writer = MediaWriter::builder().writer(Panics).fragmented_mp4().build().unwrap();

    let err = remux(input.to_str().unwrap(), writer).unwrap_err();

    assert!(matches!(&err, Error::Write(e) if e.to_string().contains("writer exploded")), "{err:?}");
}

#[test]
fn a_writer_needs_a_format() {
    let result = MediaWriter::builder().writer(io::sink()).build();
    assert!(matches!(result, Err(Error::InvalidConfig(_))));
}

#[test]
fn plain_mp4_refuses_a_writer() {
    // The mp4 muxer seeks back to write `moov` unless it fragments, so it rejects non-seekable output when it
    // initialises, before writing anything.
    let Some(input) = common::audio_sample() else { return };
    let buf = SharedBuf::default();
    let writer = MediaWriter::builder().writer(buf.clone()).format("mp4").build().unwrap();

    let err = remux(input.to_str().unwrap(), writer).unwrap_err();

    assert!(matches!(err, Error::Internal { .. }), "{err:?}");
    assert!(buf.bytes().is_empty());
}

// --- 3.3: flush -----------------------------------------------------------------------------------------------------

#[test]
fn flushing_before_each_keyframe_hands_over_whole_fragments() {
    let Some(input) = common::audio_sample() else { return };
    let input = input.to_str().unwrap();
    let video = probe(input).unwrap().video().unwrap().index;
    let buf = SharedBuf::default();
    let writer = MediaWriter::builder().writer(buf.clone()).fragmented_mp4().build().unwrap();

    // Packets written so far, and how many of them were written before the last flush.
    let (mut flushes, mut fragments, mut written, mut flushed) = (0, 0, 0, 0);
    remux_with(input, writer, |writer, packet| {
        if packet.stream_index() == video && packet.is_keyframe() {
            writer.flush()?;
            flushes += 1;
            let now = assert_fragmented(&top_level_types(&buf.bytes()));
            assert_eq!(now, fragments + usize::from(written > flushed), "flush {flushes}");
            fragments = now;
            flushed = written;
        }
        written += 1;
        Ok(())
    })
    .unwrap();

    assert!(flushes > 1);
    assert!(fragments_start_on_keyframes(&buf.bytes()));
}

#[test]
fn flush_needs_the_header() {
    let mut writer = MediaWriter::builder().writer(io::sink()).fragmented_mp4().build().unwrap();
    assert!(matches!(writer.flush(), Err(Error::InvalidConfig(_))));
}
