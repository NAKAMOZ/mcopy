//! Minimal, dependency-free logging.
//!
//! mcopy runs in places with no usable stdout: the Windows build is linked with
//! `windows_subsystem = "windows"` (no console at all), and the macOS Finder
//! Service and Linux file-manager scripts discard whatever the process prints.
//! Every diagnostic printed by earlier versions was therefore invisible exactly
//! when it mattered. This module gives those messages a durable home.
//!
//! Deliberately hand-rolled rather than pulling in `log` + a backend: the crate
//! needs one append-only file and five lines of formatting, and the project
//! targets a minimal dependency set.
//!
//! # Privacy
//!
//! Log records may contain filesystem *paths*, because a path is usually the
//! only way to explain a failure. They never contain file *contents*, clipboard
//! payloads, or environment variables. The log file is created 0600 on Unix.

use std::fmt::Display;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

/// Severity of a log record, ordered least to most severe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }

    /// Parse a `MCOPY_LOG` value. Unrecognised values fall back to the default
    /// rather than failing, so a typo never stops the app from starting.
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "debug" | "trace" => Some(Self::Debug),
            "info" => Some(Self::Info),
            "warn" | "warning" => Some(Self::Warn),
            "error" => Some(Self::Error),
            "off" | "none" => None,
            _ => Some(DEFAULT_LEVEL),
        }
    }
}

const DEFAULT_LEVEL: Level = Level::Info;

/// Keep the log bounded without a rotation scheme: once the file passes this
/// size it is truncated on the next process start. A copy tool logs a few
/// hundred bytes per run, so this holds a long history.
const MAX_LOG_BYTES: u64 = 1024 * 1024;

struct Sink {
    path: Option<PathBuf>,
    level: Option<Level>,
}

fn sink() -> &'static Sink {
    static SINK: OnceLock<Sink> = OnceLock::new();
    SINK.get_or_init(|| {
        let level = match std::env::var("MCOPY_LOG") {
            Ok(value) => Level::parse(&value),
            Err(_) => Some(DEFAULT_LEVEL),
        };

        Sink {
            path: level.and_then(|_| prepare_log_file()),
            level,
        }
    })
}

/// Resolve the platform log directory and make sure it exists.
///
/// - Windows: `%LOCALAPPDATA%\mcopy\logs`
/// - macOS: `~/Library/Logs/mcopy`
/// - Linux: `$XDG_STATE_HOME/mcopy` (falling back to `~/.local/state/mcopy`)
fn log_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir().map(|base| base.join("mcopy").join("logs"))
    }

    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|base| base.join("Library/Logs/mcopy"))
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        dirs::state_dir()
            .or_else(|| dirs::home_dir().map(|home| home.join(".local/state")))
            .map(|base| base.join("mcopy"))
    }
}

fn prepare_log_file() -> Option<PathBuf> {
    let dir = log_dir()?;
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("mcopy.log");

    // Truncate rather than rotate: one bounded file, no stale siblings.
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > MAX_LOG_BYTES) {
        let _ = std::fs::remove_file(&path);
    }

    Some(path)
}

/// Seconds since the Unix epoch, used as a compact, locale-free timestamp.
fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Append one record. Failures are swallowed: logging must never be the reason
/// a copy or an install stops.
pub fn record(level: Level, message: impl Display) {
    let sink = sink();
    let Some(minimum) = sink.level else {
        return;
    };
    if level < minimum {
        return;
    }
    let Some(path) = sink.path.as_ref() else {
        return;
    };

    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    options.mode(0o600);

    if let Ok(mut file) = options.open(path) {
        let _ = writeln!(
            file,
            "{} {:<5} [{}] {}",
            timestamp(),
            level.label(),
            std::process::id(),
            message
        );
    }
}

/// Path of the active log file, for surfacing in error messages.
pub fn log_path() -> Option<&'static PathBuf> {
    sink().path.as_ref()
}

/// Log at [`Level::Debug`]. Arguments are only formatted if the level is active.
#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        $crate::util::log::record(
            $crate::util::log::Level::Debug,
            format_args!($($arg)*),
        )
    };
}

/// Log at [`Level::Info`].
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::util::log::record(
            $crate::util::log::Level::Info,
            format_args!($($arg)*),
        )
    };
}

/// Log at [`Level::Warn`].
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        $crate::util::log::record(
            $crate::util::log::Level::Warn,
            format_args!($($arg)*),
        )
    };
}

/// Log at [`Level::Error`].
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::util::log::record(
            $crate::util::log::Level::Error,
            format_args!($($arg)*),
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_ordered_by_severity() {
        assert!(Level::Debug < Level::Info);
        assert!(Level::Info < Level::Warn);
        assert!(Level::Warn < Level::Error);
    }

    #[test]
    fn parse_accepts_known_names_case_insensitively() {
        assert_eq!(Level::parse("debug"), Some(Level::Debug));
        assert_eq!(Level::parse("INFO"), Some(Level::Info));
        assert_eq!(Level::parse(" Warn "), Some(Level::Warn));
        assert_eq!(Level::parse("error"), Some(Level::Error));
    }

    #[test]
    fn parse_disables_logging_for_off() {
        assert_eq!(Level::parse("off"), None);
        assert_eq!(Level::parse("none"), None);
    }

    #[test]
    fn parse_falls_back_to_default_for_garbage() {
        // A typo in MCOPY_LOG must not silently disable logging.
        assert_eq!(Level::parse("verbose"), Some(DEFAULT_LEVEL));
        assert_eq!(Level::parse(""), Some(DEFAULT_LEVEL));
    }

    #[test]
    fn labels_are_fixed_width_friendly() {
        for level in [Level::Debug, Level::Info, Level::Warn, Level::Error] {
            assert!(!level.label().is_empty());
            assert!(level.label().len() <= 5);
        }
    }
}
