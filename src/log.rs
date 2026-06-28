//! FFmpeg log verbosity control.
//!
//! FFmpeg's libraries (libavcodec, libx264, …) write diagnostics straight to stderr.
//! Left at FFmpeg's default level, they produce noise such as
//! `[libx264 @ …] specified frame type is not compatible with max B-frames`.
//!
//! This crate therefore **silences FFmpeg by default** ([`Level::Quiet`]). The default
//! is applied lazily the first time the crate does any FFmpeg work, so callers who do
//! nothing get clean output. To see the messages again, either:
//!
//! - call [`set_level`] before running any work, e.g.
//!   `media::log::set_level(media::Level::Debug)`, or
//! - set the `MEDIA_LOG` environment variable, e.g. `MEDIA_LOG=debug` (accepted values
//!   are the [`Level`] names, case-insensitive: `quiet`, `panic`, `fatal`, `error`,
//!   `warning`, `info`, `verbose`, `debug`, `trace`).
//!
//! An explicit [`set_level`] call always wins over the environment variable.

use crate::sys;
use std::str::FromStr;
use std::sync::Once;

/// FFmpeg log verbosity, from most to least restrictive. Maps onto FFmpeg's
/// `AV_LOG_*` levels.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Level {
    /// Print nothing (`AV_LOG_QUIET`). This is the crate default.
    Quiet,
    /// Something went so wrong the process cannot continue (`AV_LOG_PANIC`).
    Panic,
    /// Unrecoverable errors (`AV_LOG_FATAL`).
    Fatal,
    /// Errors after which processing may still continue (`AV_LOG_ERROR`).
    Error,
    /// Warnings about likely-incorrect or unexpected situations (`AV_LOG_WARNING`).
    Warning,
    /// Standard informational output (`AV_LOG_INFO`) — FFmpeg's own default.
    Info,
    /// Verbose informational output (`AV_LOG_VERBOSE`).
    Verbose,
    /// Debugging output (`AV_LOG_DEBUG`).
    Debug,
    /// Extremely verbose tracing output (`AV_LOG_TRACE`).
    Trace,
}

impl Level {
    /// The corresponding FFmpeg `AV_LOG_*` constant.
    fn as_av(self) -> i32 {
        match self {
            Level::Quiet => sys::AV_LOG_QUIET,
            Level::Panic => sys::AV_LOG_PANIC as i32,
            Level::Fatal => sys::AV_LOG_FATAL as i32,
            Level::Error => sys::AV_LOG_ERROR as i32,
            Level::Warning => sys::AV_LOG_WARNING as i32,
            Level::Info => sys::AV_LOG_INFO as i32,
            Level::Verbose => sys::AV_LOG_VERBOSE as i32,
            Level::Debug => sys::AV_LOG_DEBUG as i32,
            Level::Trace => sys::AV_LOG_TRACE as i32,
        }
    }

    /// Map a raw FFmpeg level back to the nearest [`Level`].
    fn from_av(value: i32) -> Level {
        match value {
            v if v <= sys::AV_LOG_QUIET => Level::Quiet,
            v if v <= sys::AV_LOG_PANIC as i32 => Level::Panic,
            v if v <= sys::AV_LOG_FATAL as i32 => Level::Fatal,
            v if v <= sys::AV_LOG_ERROR as i32 => Level::Error,
            v if v <= sys::AV_LOG_WARNING as i32 => Level::Warning,
            v if v <= sys::AV_LOG_INFO as i32 => Level::Info,
            v if v <= sys::AV_LOG_VERBOSE as i32 => Level::Verbose,
            v if v <= sys::AV_LOG_DEBUG as i32 => Level::Debug,
            _ => Level::Trace,
        }
    }
}

impl FromStr for Level {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "quiet" => Ok(Level::Quiet),
            "panic" => Ok(Level::Panic),
            "fatal" => Ok(Level::Fatal),
            "error" => Ok(Level::Error),
            "warning" | "warn" => Ok(Level::Warning),
            "info" => Ok(Level::Info),
            "verbose" => Ok(Level::Verbose),
            "debug" => Ok(Level::Debug),
            "trace" => Ok(Level::Trace),
            _ => Err(()),
        }
    }
}

/// Environment variable consulted by [`ensure_init`] to pick the startup level.
const ENV_VAR: &str = "MEDIA_LOG";

static INIT: Once = Once::new();

/// Apply the default verbosity exactly once, before any FFmpeg work runs.
///
/// Reads [`ENV_VAR`]; if it names a valid [`Level`], that level is applied, otherwise the crate default
/// [`Level::Quiet`] is used. Subsequent calls are no-ops, so this is cheap to call from every public entry point.
pub(crate) fn ensure_init() {
    INIT.call_once(|| {
        let level = std::env::var(ENV_VAR)
            .ok()
            .and_then(|v| v.parse::<Level>().ok())
            .unwrap_or(Level::Quiet);
        apply(level);
    });
}

/// Set the FFmpeg log verbosity, overriding both the default and `MEDIA_LOG`.
///
/// ```no_run
/// media::log::set_level(media::Level::Debug); // surface FFmpeg/libx264 messages
/// ```
pub fn set_level(level: Level) {
    // Consume the one-time init first, so a later entry point can't reset the level back to the env/default value
    // behind the caller's back.
    ensure_init();
    apply(level);
}

/// The current FFmpeg log verbosity.
pub fn level() -> Level {
    // SAFETY: `av_log_get_level` is a pure global read with no preconditions.
    Level::from_av(unsafe { sys::av_log_get_level() })
}

fn apply(level: Level) {
    // SAFETY: `av_log_set_level` is a global setter with no preconditions; the value is a valid FFmpeg level constant.
    unsafe { sys::av_log_set_level(level.as_av()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_maps_to_av_constants() {
        assert_eq!(Level::Quiet.as_av(), sys::AV_LOG_QUIET);
        assert_eq!(Level::Error.as_av(), sys::AV_LOG_ERROR as i32);
        assert_eq!(Level::Debug.as_av(), sys::AV_LOG_DEBUG as i32);
    }

    #[test]
    fn parses_names_case_insensitively() {
        assert_eq!("DEBUG".parse(), Ok(Level::Debug));
        assert_eq!("Quiet".parse(), Ok(Level::Quiet));
        assert_eq!("warn".parse(), Ok(Level::Warning));
        assert!("nonsense".parse::<Level>().is_err());
    }

    #[test]
    fn set_level_round_trips() {
        set_level(Level::Verbose);
        assert_eq!(level(), Level::Verbose);
        set_level(Level::Error);
        assert_eq!(level(), Level::Error);
    }
}
