mod session;

use crate::normalize_path;
use arboard::Clipboard;
use session::{clear_timestamp, last_copy_time, now_epoch, set_last_copy_time};
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
    clipboard.set_text(text)?;
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

    // Reuse the previous clipboard payload when the session is still fresh.
    let mut existing = if should_clear {
        Vec::new()
    } else {
        paste_paths_from_clipboard().unwrap_or_default()
    };

    // Append new paths while keeping the list unique.
    for path in paths {
        if let Ok(abs_path) = path.canonicalize().map(normalize_path)
            && !existing.contains(&abs_path)
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

    clipboard.set_text(text)?;
    set_last_copy_time();
    Ok(())
}

/// Read newline-separated paths from the clipboard and keep only existing ones.
pub fn paste_paths_from_clipboard() -> anyhow::Result<Vec<PathBuf>> {
    let mut clipboard = Clipboard::new()?;
    let text = clipboard.get_text()?;

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
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text("")?;
    // Remove the session timestamp too.
    clear_timestamp();
    Ok(())
}
