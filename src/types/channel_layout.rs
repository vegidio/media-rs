//! Channel layouts.
//!
// `ChannelLayout` and the `Channels::to_layout` helper underpin the audio re-encode path
// (the next increment); they are retained as completed, tested foundation even though the
// current pipeline stream-copies audio.
#![allow(dead_code)]
//!
//! FFmpeg 8 uses the modern [`sys::AVChannelLayout`] struct (not the legacy `u64` mask).
//! Because a custom layout can own heap memory, the struct must never be byte-copied: it is
//! initialised via `av_channel_layout_default`, duplicated via `av_channel_layout_copy`, and
//! released via `av_channel_layout_uninit`. The internal `ChannelLayout` enforces that with RAII and is
//! deliberately **not** `Copy`.

use crate::sys;
use std::mem::MaybeUninit;

/// A speaker configuration for audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channels {
    /// A single channel.
    Mono,
    /// Two channels (left/right).
    Stereo,
    /// An arbitrary channel count, laid out with FFmpeg's default layout for that count.
    Count(u32),
}

impl Channels {
    /// The number of channels.
    pub fn count(self) -> u32 {
        match self {
            Channels::Mono => 1,
            Channels::Stereo => 2,
            Channels::Count(n) => n,
        }
    }

    /// Build the owned channel layout FFmpeg expects for this configuration.
    pub(crate) fn to_layout(self) -> ChannelLayout {
        ChannelLayout::default_for(self.count() as i32)
    }
}

/// An owned, RAII channel layout wrapping [`sys::AVChannelLayout`].
pub(crate) struct ChannelLayout {
    inner: sys::AVChannelLayout,
}

impl ChannelLayout {
    /// FFmpeg's default native layout for `nb_channels` channels (e.g. 2 → stereo).
    pub(crate) fn default_for(nb_channels: i32) -> Self {
        let mut layout = MaybeUninit::<sys::AVChannelLayout>::zeroed();
        // SAFETY: av_channel_layout_default fully initialises the zeroed struct in place.
        unsafe {
            sys::av_channel_layout_default(layout.as_mut_ptr(), nb_channels);
            Self {
                inner: layout.assume_init(),
            }
        }
    }

    /// Take ownership of an existing layout by deep-copying it.
    pub(crate) fn copy_from(src: *const sys::AVChannelLayout) -> Self {
        let mut layout = MaybeUninit::<sys::AVChannelLayout>::zeroed();
        // SAFETY: src points to a valid layout; av_channel_layout_copy initialises dst.
        unsafe {
            sys::av_channel_layout_copy(layout.as_mut_ptr(), src);
            Self {
                inner: layout.assume_init(),
            }
        }
    }

    /// The number of channels.
    pub(crate) fn count(&self) -> i32 {
        self.inner.nb_channels
    }

    pub(crate) fn as_ptr(&self) -> *const sys::AVChannelLayout {
        &self.inner
    }

    /// Deep-copy this layout into `dst` (which must be uninitialised/zeroed).
    pub(crate) fn copy_into(&self, dst: *mut sys::AVChannelLayout) {
        // SAFETY: self.inner is a valid layout; dst is a writable AVChannelLayout slot.
        unsafe {
            sys::av_channel_layout_copy(dst, &self.inner);
        }
    }
}

impl Clone for ChannelLayout {
    fn clone(&self) -> Self {
        ChannelLayout::copy_from(&self.inner)
    }
}

impl Drop for ChannelLayout {
    fn drop(&mut self) {
        // SAFETY: inner was initialised by FFmpeg; uninit releases any custom-map heap.
        unsafe {
            sys::av_channel_layout_uninit(&mut self.inner);
        }
    }
}

// SAFETY: a ChannelLayout is a single owner of its (possibly heap-backed) layout with no
// shared interior state, so it is safe to send across threads.
unsafe impl Send for ChannelLayout {}
