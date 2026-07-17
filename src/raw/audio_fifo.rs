//! RAII wrapper for an `AVAudioFifo`, a sample-exact ring buffer for audio.
//!
//! Most encoders (AAC, MP3, …) demand a fixed number of samples per frame (`frame_size`),
//! while a decoder + resampler hand us arbitrary run lengths. This FIFO bridges the two: we
//! push whatever the resampler produces and pull exact `frame_size` chunks for the encoder.

use super::frame::RawFrame;
use super::util::non_null;
use crate::error::{Error, Result, check};
use crate::sys;
use std::ptr::NonNull;

/// An owned `AVAudioFifo` for one fixed sample format + channel count.
pub(crate) struct AudioFifo {
    ptr: NonNull<sys::AVAudioFifo>,
}

impl AudioFifo {
    /// Allocate a FIFO for `channels` channels of `sample_fmt`, with room for `initial` samples
    /// (it grows automatically on write).
    pub(crate) fn new(sample_fmt: sys::AVSampleFormat, channels: i32, initial: i32) -> Result<Self> {
        // SAFETY: a plain allocation; returns null on OOM.
        let ptr = unsafe { sys::av_audio_fifo_alloc(sample_fmt, channels, initial.max(1)) };
        Ok(Self { ptr: non_null(ptr, "AVAudioFifo")? })
    }

    fn as_ptr(&self) -> *mut sys::AVAudioFifo {
        self.ptr.as_ptr()
    }

    /// Number of samples (per channel) currently buffered.
    pub(crate) fn size(&self) -> i32 {
        // SAFETY: ptr is valid.
        unsafe { sys::av_audio_fifo_size(self.as_ptr()) }.max(0)
    }

    /// Append every sample in `frame` to the FIFO.
    pub(crate) fn write(&mut self, frame: &RawFrame) -> Result<()> {
        let nb = frame.nb_samples();
        if nb <= 0 {
            return Ok(());
        }
        // SAFETY: ptr is valid; the frame's plane pointers describe `nb` samples in the FIFO's
        // format; av_audio_fifo_write reads them and grows the FIFO as needed.
        let written = unsafe { sys::av_audio_fifo_write(self.as_ptr(), frame.sample_planes_ptr(), nb) };
        check(written)?;
        if written != nb {
            return Err(Error::Internal { code: 0, message: "audio FIFO short write".to_owned() });
        }
        Ok(())
    }

    /// Read `nb` samples into `dst` (which must already be allocated with capacity ≥ `nb`).
    /// Returns the number actually read.
    pub(crate) fn read(&mut self, dst: &mut RawFrame, nb: i32) -> Result<i32> {
        // SAFETY: ptr is valid; dst's planes have room for `nb` samples of the FIFO's format.
        let got = unsafe { sys::av_audio_fifo_read(self.as_ptr(), dst.sample_planes_ptr(), nb) };
        check(got)?;
        Ok(got)
    }
}

impl Drop for AudioFifo {
    fn drop(&mut self) {
        // SAFETY: av_audio_fifo_free releases the FIFO; ptr is owned.
        unsafe { sys::av_audio_fifo_free(self.as_ptr()) };
    }
}

// SAFETY: an AudioFifo uniquely owns its AVAudioFifo with no shared interior state.
unsafe impl Send for AudioFifo {}
