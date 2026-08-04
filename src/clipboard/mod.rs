//! Copy/paste state and its system-clipboard mirror.
//!
//! [`ClipboardStore`] is the source of truth; the system clipboard is a
//! best-effort interop write. That split is not an optimization: on Linux the
//! X11/Wayland selection is owned by a live process, so a copied selection
//! vanishes the moment the short-lived `mcopy copy` process exits. Only the
//! store makes copy-then-paste work across processes on every platform.

mod state;

use crate::platform;
use crate::util::path::{normalize_path, repair_shell_argument};
use crate::{log_debug, log_warn};
use arboard::Clipboard;
use std::path::{Path, PathBuf};

pub use state::{
    ClipboardState, ClipboardStore, PasteLock, SESSION_WINDOW, SessionId,
};

/// Why a paste cannot start.
///
/// Version 0.2 returned `Ok(())` for all of these, so the user right-clicked
/// Paste and nothing happened at all, with no way to tell a bug from an empty
/// clipboard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PasteRefusal {
    /// No copy has happened, or every copied source has since been deleted.
    NothingToPaste,
    /// Another paste is already running.
    AlreadyRunning,
}

impl PasteRefusal {
    pub fn message(&self) -> &'static str {
        match self {
            Self::NothingToPaste => {
                "Nothing to paste. Use \"Copy with mcopy\" first."
            },
            Self::AlreadyRunning => {
                "A paste is already in progress. Wait for it to finish."
            },
        }
    }
}

/// Store `paths` as a new copy session, replacing whatever came before.
pub fn copy_paths_to_clipboard(paths: &[PathBuf]) -> anyhow::Result<()> {
    let store = ClipboardStore::new();
    let resolved = resolve_paths(paths);

    if resolved.is_empty() {
        anyhow::bail!("No valid file paths were found to copy");
    }

    let state = store.store(&resolved)?;
    publish(&state);
    Ok(())
}

/// Extend the current copy session, or start a new one if it has expired.
///
/// Explorer and Finder invoke the copy verb once per selected item, so a
/// multi-item selection arrives as several processes in quick succession; the
/// session window is what stitches them back together.
pub fn append_paths_to_clipboard(paths: &[PathBuf]) -> anyhow::Result<()> {
    let store = ClipboardStore::new();
    let resolved = resolve_paths(paths);

    if resolved.is_empty() {
        anyhow::bail!("No valid file paths were found to append");
    }

    let state = store.append(&resolved, SESSION_WINDOW)?;
    publish(&state);
    Ok(())
}

/// The current copy state, with vanished sources already filtered out.
pub fn current_state() -> ClipboardState {
    ClipboardStore::new().load()
}

/// Forget the copy state.
pub fn clear_clipboard() -> anyhow::Result<()> {
    let store = ClipboardStore::new();
    store.clear();
    publish(&ClipboardState::Empty);
    Ok(())
}

/// Canonicalize the selection, dropping anything that cannot be resolved.
///
/// Canonicalization is what makes a copied path survive the user navigating
/// away, and it also resolves the relative paths some file managers pass.
fn resolve_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .cloned()
        // Copy verbs receive full file paths, which do not hit Explorer's
        // trailing-separator quoting artifact today — but repairing here too
        // means a future registry change cannot silently reintroduce it.
        .map(repair_shell_argument)
        .filter_map(|path| match path.canonicalize() {
            Ok(resolved) => Some(normalize_path(resolved)),
            Err(error) => {
                log_warn!("skipping {}: {error}", path.display());
                None
            },
        })
        .collect()
}

/// Mirror the state outward: system clipboard plus shell menu visibility.
///
/// Both are best-effort. A headless session or a Wayland compositor without
/// `wlr-data-control` will refuse the clipboard write, and that must not fail
/// the copy — the store already holds the authoritative state.
fn publish(state: &ClipboardState) {
    let text = state
        .items()
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n");

    match Clipboard::new() {
        Ok(mut clipboard) => {
            if let Err(error) = clipboard.set_text(text) {
                log_debug!("system clipboard write skipped: {error}");
            }
        },
        Err(error) => log_debug!("no system clipboard available: {error}"),
    }

    // Show the Paste verb exactly when pasting would do something.
    platform::sync_paste_visibility(!state.is_empty());
}

/// Begin a paste, taking the single-flight lock.
///
/// Returns the validated sources together with the lock guard and the session
/// id needed to consume the state afterwards. Holding the guard for the whole
/// paste is what stops two quick right-click Pastes from racing into the same
/// destination.
pub fn begin_paste()
-> Result<(Vec<PathBuf>, SessionId, PasteLock), PasteRefusal> {
    let store = ClipboardStore::new();

    let Some(lock) = store.try_lock_paste() else {
        return Err(PasteRefusal::AlreadyRunning);
    };

    // Load *after* taking the lock so a concurrent copy cannot slip a different
    // session in between the check and the read.
    match store.load() {
        ClipboardState::Copied { items, session, .. } => {
            Ok((items, session, lock))
        },
        ClipboardState::Empty => Err(PasteRefusal::NothingToPaste),
    }
}

/// Consume the copy state after a paste that fully succeeded.
pub fn finish_paste(session: SessionId) {
    let store = ClipboardStore::new();
    store.consume(session);
    publish(&store.load());
}

/// Re-assert menu visibility from the state on disk.
///
/// Called at startup so a stale Paste verb left by a crash, an interrupted
/// upgrade, or a reboot is corrected before the user sees it.
pub fn resync_paste_visibility() {
    platform::sync_paste_visibility(!current_state().is_empty());
}

/// Whether `target` sits inside any of `sources`.
///
/// Pasting a folder into itself would otherwise walk the copy it is making.
/// Comparison is on canonicalized paths so `..` segments and symlinks cannot be
/// used to slip past the check.
pub fn target_is_inside_sources(target: &Path, sources: &[PathBuf]) -> bool {
    let Ok(target) = target.canonicalize() else {
        return false;
    };

    sources.iter().any(|source| {
        source
            .canonicalize()
            .is_ok_and(|source| target.starts_with(&source))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_refusal_explains_itself() {
        for refusal in
            [PasteRefusal::NothingToPaste, PasteRefusal::AlreadyRunning]
        {
            assert!(!refusal.message().is_empty());
        }
        assert!(
            PasteRefusal::NothingToPaste
                .message()
                .contains("Copy with mcopy"),
            "the message should name the action that fixes it"
        );
    }

    #[test]
    fn resolve_paths_skips_entries_that_do_not_exist() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.txt");
        std::fs::write(&real, b"x").unwrap();
        let missing = dir.path().join("missing.txt");

        let resolved = resolve_paths(&[real.clone(), missing]);
        assert_eq!(resolved.len(), 1);
        assert!(resolved[0].ends_with("real.txt"));
    }

    #[test]
    fn resolve_paths_returns_nothing_for_an_all_missing_selection() {
        let dir = tempfile::tempdir().unwrap();
        assert!(resolve_paths(&[dir.path().join("nope")]).is_empty());
    }

    #[test]
    fn pasting_a_folder_into_itself_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let nested = source.join("nested");
        std::fs::create_dir_all(&nested).unwrap();

        assert!(target_is_inside_sources(
            &nested,
            std::slice::from_ref(&source)
        ));
        assert!(target_is_inside_sources(
            &source,
            std::slice::from_ref(&source)
        ));
    }

    #[test]
    fn a_sibling_destination_is_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let destination = dir.path().join("destination");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&destination).unwrap();

        assert!(!target_is_inside_sources(&destination, &[source]));
    }

    /// A `..` route back into the source must not defeat the check.
    #[test]
    fn a_traversal_path_into_the_source_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        std::fs::create_dir_all(source.join("inner")).unwrap();

        let sneaky = source.join("inner").join("..").join("inner");
        assert!(target_is_inside_sources(&sneaky, &[source]));
    }
}
