//! Compressed media packets.

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
