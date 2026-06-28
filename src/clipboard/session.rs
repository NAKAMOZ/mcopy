use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Path to the timestamp file.
fn timestamp_path() -> PathBuf {
    std::env::temp_dir().join("mcopy_session.tmp")
}

/// Read the last copy timestamp in epoch seconds.
pub(super) fn last_copy_time() -> Option<u64> {
    std::fs::read_to_string(timestamp_path())
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Persist the latest copy timestamp.
pub(super) fn set_last_copy_time() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let _ = std::fs::write(timestamp_path(), now.to_string());
}

/// Current time in epoch seconds.
pub(super) fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Remove the session timestamp file.
pub(super) fn clear_timestamp() {
    let _ = std::fs::remove_file(timestamp_path());
}
