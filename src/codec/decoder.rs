//! Stream decoding: turn compressed [`Packet`]s into decoded [`Frame`]s.

use crate::error::{Error, Result};
use crate::frame::Frame;
use crate::packet::Packet;
use crate::raw::codec_context::{drain_received, find_decoder, CodecContext};
use crate::raw::frame::RawFrame;
use crate::sys;

/// A decoder bound to one input stream.
///
/// Build one with [`StreamRef::decoder`](crate::format::reader::StreamRef::decoder). Feed it
/// packets with [`decode`](Self::decode) and drain the returned iterator; once the input is
/// exhausted, call [`flush`](Self::flush) to collect any buffered frames.
pub struct Decoder {
    ctx: CodecContext,
    recv: RawFrame,
}

impl Decoder {
    /// Build a decoder for `codec_id`, configured from a stream's parameters.
    pub(crate) fn open(
        codec_id: sys::AVCodecID,
        par: *const sys::AVCodecParameters,
    ) -> Result<Self> {
        let codec = find_decoder(codec_id);
        if codec.is_null() {
            return Err(Error::CodecUnavailable(format!("decoder for id {codec_id}")));
        }
        let mut ctx = CodecContext::alloc(codec)?;
        ctx.set_params(par)?;
        ctx.open()?;
        Ok(Self {
            ctx,
            recv: RawFrame::alloc()?,
        })
    }

    /// Submit a packet and return an iterator over the frames it produces.
    ///
    /// The iterator borrows the decoder mutably, so it must be fully drained (or dropped)
    /// before the next call — which matches FFmpeg's contract that you receive all available
    /// output before sending more input.
    pub fn decode(&mut self, packet: &Packet) -> Result<DecodeIter<'_>> {
        self.ctx.send_packet(Some(&packet.raw))?;
        Ok(DecodeIter { dec: self })
    }

    /// Flush the decoder at end of input and return any buffered frames.
    #[must_use = "the flush iterator yields the decoder's trailing frames; drain it"]
    pub fn flush(&mut self) -> Result<DecodeIter<'_>> {
        self.ctx.send_packet(None)?;
        Ok(DecodeIter { dec: self })
    }

    /// The decoded frame width in pixels (video streams).
    pub fn width(&self) -> u32 {
        self.ctx.width().max(0) as u32
    }

    /// The decoded frame height in pixels (video streams).
    pub fn height(&self) -> u32 {
        self.ctx.height().max(0) as u32
    }

    /// The decoder's output pixel format (video streams).
    pub fn pixel_format(&self) -> crate::types::pixel_format::PixelFormat {
        crate::types::pixel_format::PixelFormat::from_av(self.ctx.pix_fmt())
    }

    pub(crate) fn codec_ctx(&self) -> &CodecContext {
        &self.ctx
    }
}

/// Iterator over the frames produced by one [`Decoder::decode`]/[`Decoder::flush`] call.
#[must_use = "decoding is lazy; iterate to actually receive frames"]
pub struct DecodeIter<'d> {
    dec: &'d mut Decoder,
}

impl Iterator for DecodeIter<'_> {
    type Item = Result<Frame>;

    fn next(&mut self) -> Option<Self::Item> {
        let received = self.dec.ctx.receive_frame(&mut self.dec.recv);
        drain_received(received, || Ok(Frame::from_raw(self.dec.recv.move_out()?)))
    }
}
