use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Path to the timestamp file.
fn timestamp_path() -> PathBuf {
    std::env::temp_dir().join("mcopy_session.tmp")
}

/// Path to mcopy's own payload file.
///
/// This is the source of truth for `paste`. The system clipboard is written too
/// for interop, but on Linux a copied selection vanishes when the `copy` process
/// exits (selection-ownership model), so this file is what makes copy→paste
/// survive across processes on every platform.
fn payload_path() -> PathBuf {
    std::env::temp_dir().join("mcopy_payload.tmp")
}

/// Persist the newline-separated path payload.
pub(super) fn write_payload(text: &str) {
    let _ = std::fs::write(payload_path(), text);
}

/// Read the payload written by the last copy, if any.
pub(super) fn read_payload() -> Option<String> {
    std::fs::read_to_string(payload_path()).ok()
}

/// Remove the payload file.
pub(super) fn clear_payload() {
    let _ = std::fs::remove_file(payload_path());
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
        .unwrap_or_default()
        .as_secs();
    let _ = std::fs::write(timestamp_path(), now.to_string());
}

/// Current time in epoch seconds.
pub(super) fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Remove the session timestamp file.
pub(super) fn clear_timestamp() {
    let _ = std::fs::remove_file(timestamp_path());
}
