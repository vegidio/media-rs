//! Compressed media packets.

use crate::error::Result;
use crate::raw::packet::RawPacket;
use crate::types::rational::Rational;

/// A compressed, coded chunk of data belonging to one stream (the output of a demuxer or an
/// encoder, the input to a decoder or a muxer).
pub struct Packet {
    pub(crate) raw: RawPacket,
}

impl Packet {
    pub(crate) fn from_raw(raw: RawPacket) -> Self {
        Self { raw }
    }

    /// Build a packet from its parts: a copy of `data`, its timestamps and `duration` in the time base of the stream
    /// it will be written to, and whether it is a keyframe. Its stream index is 0 until
    /// [`set_stream_index`](Self::set_stream_index) changes it.
    ///
    /// Together with [`data`](Self::data), [`pts`](Self::pts), [`dts`](Self::dts), [`duration`](Self::duration) and
    /// [`is_keyframe`](Self::is_keyframe), this lets encoded packets be kept, in memory or on disk, and written again
    /// later:
    ///
    /// ```no_run
    /// use media::prelude::*;
    /// # fn demo(packet: &Packet) -> media::Result<()> {
    /// let kept = packet.data().to_vec();
    /// let again = Packet::from_data(&kept, packet.pts(), packet.dts(), packet.duration(), packet.is_keyframe())?;
    /// assert_eq!(again.data(), packet.data());
    /// # Ok(()) }
    /// ```
    ///
    /// Errors with [`Error::InvalidConfig`](crate::Error::InvalidConfig) when `data` is over 2 GiB, which no packet
    /// can hold.
    pub fn from_data(data: &[u8], pts: i64, dts: i64, duration: i64, keyframe: bool) -> Result<Self> {
        let mut raw = RawPacket::from_bytes(data)?;
        raw.set_timestamps(pts, dts);
        raw.set_duration(duration);
        raw.set_keyframe(keyframe);
        Ok(Self { raw })
    }

    /// The packet's compressed payload, exactly as the demuxer read it or the encoder produced it.
    pub fn data(&self) -> &[u8] {
        self.raw.data()
    }

    /// How long the packet lasts, in its stream's time base; `0` when unknown.
    pub fn duration(&self) -> i64 {
        self.raw.duration()
    }

    /// The index of the stream this packet belongs to.
    pub fn stream_index(&self) -> usize {
        self.raw.stream_index() as usize
    }

    /// Set the stream index (used when remapping input streams to output streams).
    pub fn set_stream_index(&mut self, index: usize) {
        self.raw.set_stream_index(index as i32);
    }

    /// The presentation timestamp, in the packet's stream time base.
    pub fn pts(&self) -> i64 {
        self.raw.pts()
    }

    /// The decompression timestamp, in the packet's stream time base.
    pub fn dts(&self) -> i64 {
        self.raw.dts()
    }

    /// `true` if the packet holds a keyframe: one a decoder can start from, without earlier packets.
    pub fn is_keyframe(&self) -> bool {
        self.raw.is_keyframe()
    }

    /// Rescale this packet's timestamps from `src` to `dst` time base.
    pub fn rescale_ts(&mut self, src: Rational, dst: Rational) {
        self.raw.rescale_ts(src, dst);
    }

    /// Reset the byte position so the muxer recomputes it (call before writing a remuxed
    /// packet).
    pub fn clear_pos(&mut self) {
        self.raw.clear_pos();
    }

    /// Shift this packet's pts/dts earlier by `delta` (used to re-base trimmed streams to
    /// start at zero).
    pub fn offset_timestamps(&mut self, delta: i64) {
        self.raw.shift_timestamps(delta);
    }
}
