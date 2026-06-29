mod session;

use crate::normalize_path;
use arboard::Clipboard;
use session::{
    clear_payload, clear_timestamp, last_copy_time, now_epoch, read_payload,
    set_last_copy_time, write_payload,
};
use std::collections::HashSet;
use std::path::PathBuf;

/// Write paths to the clipboard as newline-separated absolute paths.
pub fn copy_paths_to_clipboard(paths: &[PathBuf]) -> anyhow::Result<()> {
    let mut clipboard = Clipboard::new()?;

    // Canonicalize and normalize the input paths.
    let abs_paths: Vec<String> = paths
        .iter()
        .filter_map(|p| {
            p.canonicalize()
                .ok()
                .map(normalize_path)
                .and_then(|abs| abs.to_str().map(|s| s.to_string()))
        })
        .collect();

    if abs_paths.is_empty() {
        anyhow::bail!("No valid file paths were found to copy");
    }

    let text = abs_paths.join("\n");
    // mcopy's own file is the source of truth; the system clipboard is a
    // best-effort interop write (may fail headless / on Wayland w/o data-control).
    write_payload(&text);
    let _ = clipboard.set_text(text);
    set_last_copy_time();
    Ok(())
}

/// Append paths to the current clipboard payload.
/// If more than two seconds passed since the last copy, start a new session.
pub fn append_paths_to_clipboard(paths: &[PathBuf]) -> anyhow::Result<()> {
    const SESSION_TIMEOUT_SECS: u64 = 2;

    // Decide whether the previous copy session is still active.
    let should_clear = match last_copy_time() {
        Some(last_time) => now_epoch() - last_time > SESSION_TIMEOUT_SECS,
        None => true,
    };

    // Reuse the previous payload when the session is still fresh. The payload
    // file is authoritative; fall back to the system clipboard if it's gone.
    let mut existing = if should_clear {
        Vec::new()
    } else {
        paste_paths_from_clipboard().unwrap_or_default()
    };

    // Append new paths while keeping the list unique. A HashSet tracks seen
    // paths in O(1) so the dedup check is not a linear scan per iteration.
    let mut seen: HashSet<PathBuf> = existing.iter().cloned().collect();
    for path in paths {
        if let Ok(abs_path) = path.canonicalize().map(normalize_path)
            && seen.insert(abs_path.clone())
        {
            existing.push(abs_path);
        }
    }

    if existing.is_empty() {
        anyhow::bail!("No valid file paths were found to append");
    }

    // Write the merged payload back into the clipboard.
    let mut clipboard = Clipboard::new()?;
    let text = existing
        .iter()
        .filter_map(|p| p.to_str())
        .collect::<Vec<_>>()
        .join("\n");

    write_payload(&text);
    let _ = clipboard.set_text(text);
    set_last_copy_time();
    Ok(())
}

/// Read newline-separated paths and keep only existing ones.
///
/// mcopy's own payload file is read first so copy→paste works even where the
/// system clipboard didn't survive the `copy` process exiting (Linux). The
/// system clipboard is only consulted when the payload file is absent/empty.
pub fn paste_paths_from_clipboard() -> anyhow::Result<Vec<PathBuf>> {
    let text = match read_payload() {
        Some(text) if !text.trim().is_empty() => text,
        _ => {
            let mut clipboard = Clipboard::new()?;
            clipboard.get_text().unwrap_or_default()
        },
    };

    if text.trim().is_empty() {
        return Ok(Vec::new());
    }

    let paths: Vec<PathBuf> = text
        .lines()
        .map(|line| PathBuf::from(line.trim()))
        .filter(|p| p.exists())
        .collect();

    Ok(paths)
}

/// Clear the clipboard payload.
pub fn clear_clipboard() -> anyhow::Result<()> {
    // Drop our own payload (source of truth) and the session timestamp.
    clear_payload();
    clear_timestamp();
    // Best-effort clear of the system clipboard too.
    if let Ok(mut clipboard) = Clipboard::new() {
        let _ = clipboard.set_text("");
    }
    Ok(())
}
