//! Encode a video as independent 2-second H.264 segments, keep them as plain bytes, then write them out as one
//! fragmented MP4. Each segment comes from a fresh encoder and starts on its own keyframe, so any one of them can be
//! re-encoded, cached or served on its own: what a player that transcodes on demand needs.
//!
//! Shows: `MediaReader::stream_rotation`, `Decoder::sample_aspect_ratio`, a public `VideoFilter`,
//! `VideoEncoderBuilder::option`, and `Packet::data`/`Packet::from_data`.
//!
//! Run with: `cargo run --example segments`

use media::prelude::*;

const INPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/video2.mp4");
const OUTPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/temp_segments.mp4");
const SEGMENT_SECS: f64 = 2.0;

/// An encoded packet, kept as plain data: all a muxer needs to write it again.
struct Kept {
    data: Vec<u8>,
    pts: i64,
    dts: i64,
    duration: i64,
    keyframe: bool,
}

impl From<&Packet> for Kept {
    fn from(packet: &Packet) -> Self {
        let (pts, dts, duration, keyframe) = (packet.pts(), packet.dts(), packet.duration(), packet.is_keyframe());
        Self { data: packet.data().to_vec(), pts, dts, duration, keyframe }
    }
}

/// Settings every segment's encoder shares, so their packets fit under one init segment.
#[derive(Clone, Copy)]
struct Settings {
    width: u32,
    height: u32,
    frame_rate: Framerate,
    time_base: Rational,
}

impl Settings {
    /// A fresh encoder: exactly one keyframe, its first frame (a GOP longer than a segment, no scene cuts), and no
    /// B-frames, so nothing ties a segment to the one before it.
    fn encoder(self) -> media::Result<VideoEncoder> {
        VideoEncoder::builder()
            .codec(VideoCodec::H264)
            .resolution(self.width, self.height)
            .framerate(self.frame_rate)
            .time_base(self.time_base)
            .preset(H264Preset::Veryfast)
            .gop_size(1000)
            .option("bf", "0")
            .option("sc_threshold", "0")
            .build()
    }
}

/// Cuts decoded frames into segments by their timestamp, encoding each segment with its own encoder.
struct Segmenter {
    settings: Settings,
    filter: VideoFilter,
    segment_ticks: i64,
    current: Option<(i64, VideoEncoder, Vec<Kept>)>,
    done: Vec<Vec<Kept>>,
}

impl Segmenter {
    fn push(&mut self, mut frame: Frame) -> media::Result<()> {
        let pts = frame.best_effort_timestamp().unwrap_or(0);
        frame.set_pts(pts);
        let segment = pts / self.segment_ticks;
        if self.current.as_ref().is_some_and(|(at, ..)| *at != segment) {
            self.finish()?;
        }
        if self.current.is_none() {
            self.current = Some((segment, self.settings.encoder()?, Vec::new()));
        }
        let (_, encoder, kept) = self.current.as_mut().expect("just set");
        for shaped in self.filter.filter(frame)? {
            for packet in encoder.encode(&shaped)? {
                kept.push(Kept::from(&packet?));
            }
        }
        Ok(())
    }

    /// Flushes the current segment's encoder and keeps the segment.
    fn finish(&mut self) -> media::Result<()> {
        if let Some((_, mut encoder, mut kept)) = self.current.take() {
            for packet in encoder.flush()? {
                kept.push(Kept::from(&packet?));
            }
            self.done.push(kept);
        }
        Ok(())
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut reader = MediaReader::open(INPUT)?;
    let index = reader.best_stream(StreamKind::Video)?;
    let time_base = reader.stream_time_base(index)?;
    let frame_rate = Framerate(reader.stream_avg_frame_rate(index)?);
    let rotation = reader.stream_rotation(index)?;
    let mut decoder = reader.stream(index).decoder()?;

    // The shape the picture is shown at: pixels made square, then turned upright. Encode it at half that size.
    let mut shown = (f64::from(decoder.width()) * decoder.sample_aspect_ratio().as_f64(), f64::from(decoder.height()));
    let turn = match rotation {
        Some(90) => "transpose=clock,",
        Some(180) => "hflip,vflip,",
        Some(270) => "transpose=cclock,",
        _ => "",
    };
    if matches!(rotation, Some(90 | 270)) {
        shown = (shown.1, shown.0);
    }
    let even = |side: f64| (side / 4.0).round() as u32 * 2;
    let chain = VideoFilterChain::raw(format!("{turn}scale={}:{},format=yuv420p", even(shown.0), even(shown.1)));
    let filter = VideoFilter::new(&decoder, time_base, &chain)?;

    let settings = Settings { width: filter.output_width(), height: filter.output_height(), frame_rate, time_base };
    let segment_ticks = (SEGMENT_SECS / time_base.as_f64()).round() as i64;
    let mut segmenter = Segmenter { settings, filter, segment_ticks, current: None, done: Vec::new() };

    for packet in reader.packets() {
        let packet = packet?;
        if packet.stream_index() != index {
            continue;
        }
        for frame in decoder.decode(&packet)? {
            segmenter.push(frame?)?;
        }
    }
    for frame in decoder.flush()? {
        segmenter.push(frame?)?;
    }
    segmenter.finish()?;

    // The kept segments are plain bytes now. Write them back as fragmented MP4, one fragment per segment, under the
    // init segment of an encoder with the same settings.
    let mut writer = MediaWriter::builder().path(OUTPUT).fragmented_mp4().build()?;
    let stream = writer.add_stream_from_encoder(&settings.encoder()?)?;
    writer.write_header()?;
    for (n, segment) in segmenter.done.iter().enumerate() {
        let keyframes = segment.iter().filter(|kept| kept.keyframe).count();
        println!("segment {n}: {} packets, {keyframes} keyframe", segment.len());
        for kept in segment {
            let mut packet = Packet::from_data(&kept.data, kept.pts, kept.dts, kept.duration, kept.keyframe)?;
            packet.set_stream_index(stream);
            writer.write_packet(&mut packet)?;
        }
        writer.flush()?;
    }
    writer.write_trailer()?;
    println!("Wrote {OUTPUT}: {}x{}", settings.width, settings.height);

    Ok(())
}
