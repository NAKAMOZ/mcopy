use std::fs::{DirBuilder, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

/// Per-user, private directory for mcopy's session files.
///
/// Deliberately kept out of the world-writable system temp dir: the payload file
/// drives `paste` (a real file copy), so a well-known path in `/tmp` would let
/// another local user pre-plant a symlink (turning our write into an arbitrary
/// overwrite) or pre-seed attacker-chosen source paths. On Linux this resolves
/// to `$XDG_RUNTIME_DIR` (already 0700 and per-user); elsewhere a per-user local
/// data dir; only as a last resort the system temp dir. The directory is created
/// 0700 on Unix so its contents aren't reachable by other users.
fn session_dir() -> PathBuf {
    let base = dirs::runtime_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("mcopy");

    let mut builder = DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    let _ = builder.create(&dir);

    dir
}

/// Path to the timestamp file.
fn timestamp_path() -> PathBuf {
    session_dir().join("session.tmp")
}

/// Path to mcopy's own payload file.
///
/// This is the source of truth for `paste`. The system clipboard is written too
/// for interop, but on Linux a copied selection vanishes when the `copy` process
/// exits (selection-ownership model), so this file is what makes copy→paste
/// survive across processes on every platform.
fn payload_path() -> PathBuf {
    session_dir().join("payload.tmp")
}

/// Write a private (0600 on Unix) session file, truncating any prior contents.
fn write_private(path: &Path, contents: &str) {
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    opts.mode(0o600);

    if let Ok(mut file) = opts.open(path) {
        let _ = file.write_all(contents.as_bytes());
    }
}

/// Read the last copy timestamp in epoch seconds.
pub(super) fn last_copy_time() -> Option<u64> {
    std::fs::read_to_string(timestamp_path())
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Persist the latest copy timestamp.
pub(super) fn set_last_copy_time() {
    write_private(&timestamp_path(), &now_epoch().to_string());
}

/// Persist the newline-separated path payload.
pub(super) fn write_payload(text: &str) {
    write_private(&payload_path(), text);
}

/// Read the payload written by the last copy, if any.
pub(super) fn read_payload() -> Option<String> {
    std::fs::read_to_string(payload_path()).ok()
}

/// Remove the payload file.
pub(super) fn clear_payload() {
    let _ = std::fs::remove_file(payload_path());
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
