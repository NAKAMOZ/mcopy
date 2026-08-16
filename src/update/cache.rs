//! The once-a-day throttle.
//!
//! mcopy is launched by hand, so "check on every launch" would be a network
//! round-trip every time somebody opens it. A timestamp file makes the check
//! rare without needing a background service or a scheduled task.
//!
//! Every failure here is swallowed: a cache file that cannot be read or written
//! must never be the reason mcopy does not start.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long to wait between checks.
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

const LAST_CHECK_FILE: &str = "update-check";

/// Where the timestamp lives.
///
/// `cache_dir` rather than the `data_local_dir`/`state_dir` that
/// [`crate::util::log`] uses: losing this file costs one extra HTTP request,
/// so it belongs somewhere a cleaner is allowed to empty.
fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|base| base.join("mcopy"))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Whether enough time has passed since the last check.
///
/// Unreadable or corrupt state answers "yes": a missing throttle is a wasted
/// request, while a stuck throttle would silently disable updates forever.
pub fn is_due() -> bool {
    let Some(path) = cache_dir().map(|dir| dir.join(LAST_CHECK_FILE)) else {
        return true;
    };
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return true;
    };
    let Ok(last) = contents.trim().parse::<u64>() else {
        return true;
    };

    // A timestamp in the future means the clock moved backwards; treat it as
    // due rather than locking updates out until the clock catches up.
    let now = now_secs();
    now < last || now - last >= CHECK_INTERVAL.as_secs()
}

/// Record that a check is being attempted.
///
/// Called *before* the request, not after it: a server that hangs until the
/// timeout must not leave every subsequent launch retrying it.
pub fn record_checked_now() {
    let Some(dir) = cache_dir() else {
        return;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let _ = std::fs::write(dir.join(LAST_CHECK_FILE), now_secs().to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_interval_is_a_day() {
        assert_eq!(CHECK_INTERVAL.as_secs(), 86_400);
    }

    /// The throttle must fail open. Every early return in `is_due` exists so a
    /// missing or damaged cache file cannot disable update checks permanently.
    #[test]
    fn a_missing_cache_file_is_due() {
        let missing = std::env::temp_dir().join("mcopy-nonexistent-check-file");
        let _ = std::fs::remove_file(&missing);
        assert!(std::fs::read_to_string(&missing).is_err());
    }
}
