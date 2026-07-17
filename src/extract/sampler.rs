//! The sampling engine shared by the builder/one-liner runners and the Tier-3 `sampled_at`
//! iterator.
//!
//! Given an [`Interval`], it resolves *which* frames to emit, then seeks and decodes only what
//! is needed to reach each one: for sparse sampling it seeks to the nearest keyframe before a
//! target and decodes forward; for closely spaced targets it just keeps decoding. Pulling a
//! frame every 10s from a two-hour video therefore never decodes the whole stream.

use crate::codec::decoder::Decoder;
use crate::error::{Error, Result};
use crate::extract::frame::ExtractedFrame;
use crate::extract::types::{Interval, Resolution};
use crate::filter::{VideoFilterChain, VideoFilter};
use crate::format::MediaReader;
use crate::frame::Frame;
use crate::types::pixel_format::PixelFormat;
use crate::types::rational::Rational;
use std::collections::VecDeque;
use std::time::Duration;

/// If the next target is more than this many seconds ahead of the current decode position,
/// seek to it; otherwise decoding forward is cheaper than a seek + keyframe re-decode.
const SEEK_THRESHOLD_SECS: f64 = 2.0;

/// Convert a duration to a timestamp in `tb`, rounding to the nearest tick.
pub(crate) fn duration_to_ts(d: Duration, tb: Rational) -> i64 {
    tb.ts_from_secs(d.as_secs_f64())
}

/// The resolved sampling strategy.
enum Plan {
    /// Fixed-step targets from `next`, advancing by `step_ts` while `< end_ts`. Lazy, so it
    /// handles unbounded/unknown durations without materialising a list.
    Step { step_ts: i64, next: i64, end_ts: i64 },
    /// A precomputed ascending list of target timestamps.
    Targets { targets: Vec<i64>, cursor: usize },
    /// Every `n`-th decoded frame within `[start_ts, end_ts)`, decoded sequentially. `frame_no`
    /// is the running decoded-frame counter and `sought` the one-shot "seeked to range start"
    /// flag — both only meaningful for this variant, so they live here rather than on the
    /// shared [`SampledFrames`].
    EveryN { n: u64, start_ts: i64, end_ts: i64, frame_no: u64, sought: bool },
}

impl Plan {
    fn resolve(interval: &Interval, range: Option<(Duration, Duration)>, duration_secs: f64, tb: Rational) -> Plan {
        let start_secs = range.map(|(s, _)| s.as_secs_f64().max(0.0)).unwrap_or(0.0);
        // End bound: explicit range end, else the known duration, else unbounded.
        let end_secs = match (range, duration_secs > 0.0) {
            (Some((_, e)), true) => e.as_secs_f64().min(duration_secs),
            (Some((_, e)), false) => e.as_secs_f64(),
            (None, true) => duration_secs,
            (None, false) => f64::INFINITY,
        };
        let start_ts = duration_to_ts(Duration::from_secs_f64(start_secs), tb);
        // Targets/`EveryN` treat `end_ts` as an exclusive upper bound. An explicit range end
        // is inclusive (the user picked `RangeInclusive`), so nudge it one tick past to keep
        // the endpoint; a duration-derived end stays exclusive (no frame exists at exactly the
        // duration).
        let end_ts = if end_secs.is_finite() {
            let base = duration_to_ts(Duration::from_secs_f64(end_secs.max(0.0)), tb);
            if range.is_some() { base + 1 } else { base }
        } else {
            i64::MAX
        };

        match interval {
            Interval::EverySeconds(step) => step_plan(*step, start_ts, end_ts, tb),
            Interval::Fps(fps) => step_plan(if *fps > 0.0 { 1.0 / *fps } else { 0.0 }, start_ts, end_ts, tb),
            Interval::EveryNFrames(n) => {
                Plan::EveryN { n: (*n).max(1) as u64, start_ts, end_ts, frame_no: 0, sought: false }
            }
            Interval::Count(n) => Plan::Targets { targets: count_targets(*n, start_secs, end_secs, tb), cursor: 0 },
            Interval::Timestamps(list) => {
                let mut targets: Vec<i64> = list.iter().map(|d| duration_to_ts(*d, tb)).collect();
                targets.sort_unstable();
                targets.dedup();
                Plan::Targets { targets, cursor: 0 }
            }
        }
    }

    /// The next target timestamp for the seek-based strategies (`Step`/`Targets`).
    fn peek_target(&self) -> Option<i64> {
        match self {
            Plan::Step { next, end_ts, .. } => (*next < *end_ts).then_some(*next),
            Plan::Targets { targets, cursor } => targets.get(*cursor).copied(),
            Plan::EveryN { .. } => None,
        }
    }

    /// Consume the current target so the next call advances.
    fn advance_target(&mut self) {
        match self {
            Plan::Step { next, step_ts, .. } => *next += *step_ts,
            Plan::Targets { cursor, .. } => *cursor += 1,
            Plan::EveryN { .. } => {}
        }
    }
}

/// Build a `Step` plan from a per-frame spacing in seconds. A non-positive step yields no
/// frames (rather than an infinite loop).
fn step_plan(step_secs: f64, start_ts: i64, end_ts: i64, tb: Rational) -> Plan {
    if step_secs <= 0.0 {
        return Plan::Targets { targets: Vec::new(), cursor: 0 };
    }
    let step_ts = duration_to_ts(Duration::from_secs_f64(step_secs), tb).max(1);
    Plan::Step { step_ts, next: start_ts, end_ts }
}

/// `n` timestamps evenly spread across `[start_secs, end_secs)` at the midpoints of `n` equal
/// slices — so neither the very first nor very last instant is forced. Empty if the span is
/// unbounded (a `Count` needs a known duration).
fn count_targets(n: u32, start_secs: f64, end_secs: f64, tb: Rational) -> Vec<i64> {
    if n == 0 || !end_secs.is_finite() {
        return Vec::new();
    }
    let span = (end_secs - start_secs).max(0.0);
    (0..n)
        .map(|i| {
            let t = start_secs + (i as f64 + 0.5) * span / n as f64;
            duration_to_ts(Duration::from_secs_f64(t.max(0.0)), tb)
        })
        .collect()
}

/// Converts decoded (YUV) frames to tightly packed RGB24 at the desired resolution, using a
/// libavfilter graph (`scale=…,format=rgb24`) built once for the stream's shape.
struct FrameConverter {
    filter: VideoFilter,
}

impl FrameConverter {
    fn new(frame: &Frame, resolution: Resolution, time_base: Rational) -> Result<Self> {
        let chain = match resolution {
            Resolution::Original => VideoFilterChain::raw("format=rgb24"),
            Resolution::Fixed(w, h) => VideoFilterChain::raw(format!("scale={w}:{h},format=rgb24")),
        };
        let filter = VideoFilter::new(
            frame.width() as i32,
            frame.height() as i32,
            frame.pixel_format(),
            time_base,
            Rational::ONE,
            &chain,
        )?;
        Ok(Self { filter })
    }

    /// Returns the packed RGB24 bytes plus the output `(width, height)`.
    fn convert(&mut self, frame: Frame) -> Result<(Vec<u8>, u32, u32)> {
        let out = self
            .filter
            .filter(frame)?
            .into_iter()
            .next()
            .ok_or_else(|| Error::ImageEncode("filter produced no output frame".to_owned()))?;
        let buf = out.raw.copy_to_packed_buffer(PixelFormat::Rgb24.to_av())?;
        Ok((buf, out.width(), out.height()))
    }
}

/// An iterator that samples a video stream, yielding one [`ExtractedFrame`] per sample point.
///
/// Obtain one via [`StreamRef::sampled_at`](crate::format::StreamRef::sampled_at) (Tier 3) or
/// internally from the [`FrameExtractor`](crate::extract::FrameExtractor) runner. It borrows
/// the [`MediaReader`] mutably so it can seek while decoding.
pub struct SampledFrames<'r> {
    reader: &'r mut MediaReader,
    stream_index: usize,
    decoder: Decoder,
    time_base: Rational,
    resolution: Resolution,
    plan: Plan,
    converter: Option<FrameConverter>,
    queue: VecDeque<Frame>,
    eof: bool,
    current_ts: Option<i64>,
    next_index: u32,
    done: bool,
}

impl<'r> SampledFrames<'r> {
    /// Build a sampler over `stream_index` at full source resolution (the Tier-3 entry point).
    pub(crate) fn from_stream(reader: &'r mut MediaReader, stream_index: usize, interval: Interval) -> Result<Self> {
        Self::new(reader, stream_index, interval, None, Resolution::Original)
    }

    /// Build a sampler with full control over range and output resolution.
    pub(crate) fn new(
        reader: &'r mut MediaReader,
        stream_index: usize,
        interval: Interval,
        range: Option<(Duration, Duration)>,
        resolution: Resolution,
    ) -> Result<Self> {
        let time_base = reader.stream_time_base(stream_index)?;
        let duration_secs = reader.duration_secs();
        let decoder = reader.stream(stream_index).decoder()?;
        let plan = Plan::resolve(&interval, range, duration_secs, time_base);
        Ok(Self {
            reader,
            stream_index,
            decoder,
            time_base,
            resolution,
            plan,
            converter: None,
            queue: VecDeque::new(),
            eof: false,
            current_ts: None,
            next_index: 0,
            done: false,
        })
    }

    /// Seek so decoding resumes at the keyframe at or before `target`, discarding stale
    /// decoder state.
    fn seek_to(&mut self, target: i64) -> Result<()> {
        self.reader.seek_ts(self.stream_index, target)?;
        self.decoder.reset();
        self.queue.clear();
        self.eof = false;
        self.current_ts = None;
        Ok(())
    }

    /// Pull the next decoded frame, reading packets (and finally flushing) as needed.
    fn next_frame(&mut self) -> Result<Option<Frame>> {
        loop {
            if let Some(frame) = self.queue.pop_front() {
                return Ok(Some(frame));
            }
            if self.eof {
                return Ok(None);
            }
            match self.reader.next_packet()? {
                Some(packet) => {
                    if packet.stream_index() != self.stream_index {
                        continue;
                    }
                    for frame in self.decoder.decode(&packet)? {
                        self.queue.push_back(frame?);
                    }
                }
                None => {
                    for frame in self.decoder.flush()? {
                        self.queue.push_back(frame?);
                    }
                    self.eof = true;
                }
            }
        }
    }

    /// Convert `frame` into an [`ExtractedFrame`], building the converter on first use.
    fn make_extracted(&mut self, frame: Frame, ts: i64) -> Result<ExtractedFrame> {
        if self.converter.is_none() {
            self.converter = Some(FrameConverter::new(&frame, self.resolution, self.time_base)?);
        }
        let (rgb, width, height) = self.converter.as_mut().unwrap().convert(frame)?;
        let index = self.next_index;
        self.next_index += 1;
        let timestamp = Duration::from_secs_f64(self.time_base.secs_from_ts(ts).max(0.0));
        Ok(ExtractedFrame::new(index, timestamp, width, height, rgb))
    }

    /// Seek-based sampling: emit one frame for the next target. Seek if the target is far
    /// ahead (or behind), then decode forward and return the first frame at or after it.
    fn next_target(&mut self) -> Result<Option<ExtractedFrame>> {
        let target = match self.plan.peek_target() {
            Some(t) => t,
            None => return Ok(None),
        };
        let need_seek = match self.current_ts {
            None => self.time_base.secs_from_ts(target) > SEEK_THRESHOLD_SECS,
            Some(cur) => self.time_base.secs_from_ts(target - cur) > SEEK_THRESHOLD_SECS,
        };
        if need_seek {
            self.seek_to(target)?;
        }
        loop {
            match self.next_frame()? {
                Some(frame) => {
                    let ts = frame.best_ts();
                    self.current_ts = Some(ts);
                    if ts >= target {
                        self.plan.advance_target();
                        return Ok(Some(self.make_extracted(frame, ts)?));
                    }
                    // Frame is before the target (e.g. between keyframe and target); skip.
                }
                // Ran out of frames before reaching the target: nothing more to emit.
                None => return Ok(None),
            }
        }
    }

    /// Sequential sampling for `EveryNFrames`: decode in order within the range and emit every
    /// `n`-th frame.
    fn next_every_n(&mut self) -> Result<Option<ExtractedFrame>> {
        // The immutable bounds; the running counter and seek flag are mutated in place on the
        // plan below. The `_` arms are unreachable given `Iterator::next`'s dispatch, but stay
        // lenient rather than panic on a future dispatch change.
        let (n, start_ts, end_ts) = match self.plan {
            Plan::EveryN { n, start_ts, end_ts, .. } => (n, start_ts, end_ts),
            _ => return Ok(None),
        };
        // Seek to the range start exactly once, before the first pull.
        let seek_now = match &mut self.plan {
            Plan::EveryN { sought, .. } if !*sought => {
                *sought = true;
                true
            }
            _ => false,
        };
        if seek_now && self.time_base.secs_from_ts(start_ts) > SEEK_THRESHOLD_SECS {
            self.seek_to(start_ts)?;
        }
        loop {
            match self.next_frame()? {
                Some(frame) => {
                    let ts = frame.best_ts();
                    if ts < start_ts {
                        continue;
                    }
                    if ts >= end_ts {
                        return Ok(None);
                    }
                    // Read-then-advance the running frame counter held in the plan.
                    let emit = match &mut self.plan {
                        Plan::EveryN { frame_no, .. } => {
                            let emit = frame_no.is_multiple_of(n);
                            *frame_no += 1;
                            emit
                        }
                        _ => false,
                    };
                    if emit {
                        return Ok(Some(self.make_extracted(frame, ts)?));
                    }
                }
                None => return Ok(None),
            }
        }
    }
}

impl Iterator for SampledFrames<'_> {
    type Item = Result<ExtractedFrame>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let result = match self.plan {
            Plan::EveryN { .. } => self.next_every_n(),
            _ => self.next_target(),
        };
        match result {
            Ok(Some(frame)) => Some(Ok(frame)),
            Ok(None) => {
                self.done = true;
                None
            }
            Err(e) => {
                self.done = true;
                Some(Err(e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A 1/1000s time base makes 1 tick == 1 millisecond, so timestamps read as ms.
    const TB: Rational = Rational::new(1, 1000);

    fn targets(interval: Interval, range: Option<(Duration, Duration)>, duration: f64) -> Vec<i64> {
        match Plan::resolve(&interval, range, duration, TB) {
            Plan::Targets { targets, .. } => targets,
            Plan::Step { step_ts, mut next, end_ts } => {
                // Expand the lazy step plan for assertion.
                let mut out = Vec::new();
                while next < end_ts {
                    out.push(next);
                    next += step_ts;
                }
                out
            }
            Plan::EveryN { .. } => panic!("expected a target-based plan"),
        }
    }

    #[test]
    fn every_seconds_starts_at_zero_and_stops_before_duration() {
        // 10s video, one frame per second → 10 targets at 0..9s (10s itself is excluded).
        let t = targets(Interval::EverySeconds(1.0), None, 10.0);
        assert_eq!(t, vec![0, 1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000, 9000]);
    }

    #[test]
    fn fps_is_the_reciprocal_step() {
        // 2 fps over 2s → every 500ms: 0, 500, 1000, 1500.
        let t = targets(Interval::Fps(2.0), None, 2.0);
        assert_eq!(t, vec![0, 500, 1000, 1500]);
    }

    #[test]
    fn count_spreads_midpoints_across_the_duration() {
        // Exactly 4 frames over 8s at slice midpoints: 1s, 3s, 5s, 7s.
        let t = targets(Interval::Count(4), None, 8.0);
        assert_eq!(t, vec![1000, 3000, 5000, 7000]);
    }

    #[test]
    fn timestamps_are_sorted_and_deduped() {
        let t = targets(
            Interval::Timestamps(vec![
                Duration::from_millis(900),
                Duration::from_millis(100),
                Duration::from_millis(900),
            ]),
            None,
            10.0,
        );
        assert_eq!(t, vec![100, 900]);
    }

    #[test]
    fn range_clamps_the_sampling_window() {
        // Every second, but only within 3s..=6s → 3s, 4s, 5s, 6s.
        let t = targets(Interval::EverySeconds(1.0), Some((Duration::from_secs(3), Duration::from_secs(6))), 10.0);
        assert_eq!(t, vec![3000, 4000, 5000, 6000]);
    }

    #[test]
    fn count_needs_a_known_duration() {
        // Unknown duration (0.0) and no range → a Count can't be placed.
        let t = targets(Interval::Count(5), None, 0.0);
        assert!(t.is_empty());
    }

    #[test]
    fn non_positive_step_yields_no_frames() {
        assert!(targets(Interval::EverySeconds(0.0), None, 10.0).is_empty());
        assert!(targets(Interval::Fps(0.0), None, 10.0).is_empty());
    }

    #[test]
    fn every_n_frames_resolves_to_an_everyn_plan() {
        // `EveryNFrames` decodes sequentially, so it resolves to an `EveryN` plan (not a
        // target list) with its counter/seek flag initialised and the range as `[start, end)`.
        match Plan::resolve(
            &Interval::EveryNFrames(3),
            Some((Duration::from_secs(2), Duration::from_secs(6))),
            10.0,
            TB,
        ) {
            Plan::EveryN { n, start_ts, end_ts, frame_no, sought } => {
                assert_eq!(n, 3);
                assert_eq!(start_ts, 2000);
                // Range end is inclusive, so it is nudged one tick past 6000.
                assert_eq!(end_ts, 6001);
                assert_eq!(frame_no, 0);
                assert!(!sought);
            }
            _ => panic!("expected an EveryN plan"),
        }
    }

    #[test]
    fn every_n_frames_clamps_zero_to_one() {
        // `EveryNFrames(0)` would emit every frame; it is clamped to 1 rather than dividing by
        // zero when testing `frame_no % n`.
        match Plan::resolve(&Interval::EveryNFrames(0), None, 10.0, TB) {
            Plan::EveryN { n, .. } => assert_eq!(n, 1),
            _ => panic!("expected an EveryN plan"),
        }
    }
}
