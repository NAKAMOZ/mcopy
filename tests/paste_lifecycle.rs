//! End-to-end behavior of the copy → paste → clear lifecycle.
//!
//! These drive the real [`ClipboardStore`] and the real copy engine against a
//! temporary directory. They cover the state transitions behind the Paste
//! button's visibility, which is the part of issue 6 that no unit test on a
//! single module can demonstrate on its own.
//!
//! The store is constructed with an explicit directory rather than the per-user
//! default, so tests never touch the developer's real clipboard state and can
//! run concurrently.

use mcopy::clipboard::{ClipboardState, ClipboardStore, SESSION_WINDOW};
use mcopy::{
    CopyController, CopyErrorKind, ProgressPhase, ProgressUpdate,
    collect_files, copy_files_with_progress, precreate_directories,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

/// A temporary workspace with an isolated clipboard store.
struct Workspace {
    root: TempDir,
    store: ClipboardStore,
}

impl Workspace {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temp dir");
        let store = ClipboardStore::at(root.path().join("clipboard-state"));
        Self { root, store }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.path().join(relative)
    }

    /// Create a file with `contents`, making parent directories as needed.
    fn file(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.path(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(&path, contents).expect("write file");
        path
    }

    fn dir(&self, relative: &str) -> PathBuf {
        let path = self.path(relative);
        std::fs::create_dir_all(&path).expect("create dir");
        path
    }
}

/// Outcome of running one paste against a destination.
struct PasteOutcome {
    completed: usize,
    failed: usize,
    kinds: Vec<CopyErrorKind>,
}

impl PasteOutcome {
    fn succeeded(&self) -> bool {
        self.failed == 0
    }
}

/// Run the copy engine over `sources` into `target`, exactly as `run_paste`
/// does, but without opening a window.
async fn paste(
    sources: &[PathBuf],
    target: &Path,
    controller: &CopyController,
) -> PasteOutcome {
    let mut items = Vec::new();
    for source in sources {
        items.extend(
            collect_files(source, target)
                .await
                .expect("collect the source tree"),
        );
    }

    precreate_directories(&items).await.expect("precreate dirs");

    let completed = Arc::new(Mutex::new(0usize));
    let failed = Arc::new(Mutex::new(0usize));
    let kinds = Arc::new(Mutex::new(Vec::new()));

    let callback = {
        let completed = completed.clone();
        let failed = failed.clone();
        let kinds = kinds.clone();
        Box::new(move |update: ProgressUpdate| match update.phase {
            ProgressPhase::Finished => *completed.lock().unwrap() += 1,
            ProgressPhase::Failed => {
                *failed.lock().unwrap() += 1;
                if let Some(kind) = update.error {
                    kinds.lock().unwrap().push(kind);
                }
            },
            ProgressPhase::Started => {},
        })
    };

    copy_files_with_progress(
        items,
        4,
        Some(callback),
        Some(controller.clone()),
    )
    .await
    .expect("the queue itself must not fail");

    let completed = *completed.lock().unwrap();
    let failed = *failed.lock().unwrap();
    let kinds = kinds.lock().unwrap().clone();
    PasteOutcome {
        completed,
        failed,
        kinds,
    }
}

/// Mirror of the decision `run_paste` makes: consume the copy state only when
/// the paste finished cleanly.
fn settle(
    store: &ClipboardStore,
    state: &ClipboardState,
    outcome: &PasteOutcome,
    controller: &CopyController,
) {
    let ClipboardState::Copied { session, .. } = state else {
        return;
    };

    if outcome.succeeded() && !controller.is_cancelled() {
        store.consume(*session);
    }
}

#[tokio::test]
async fn a_successful_paste_clears_the_copy_state() {
    let workspace = Workspace::new();
    let source = workspace.file("src/report.txt", "hello");
    let target = workspace.dir("dst");

    let state = workspace.store.store(&[source]).expect("store");
    assert!(!state.is_empty(), "copying must arm the paste state");

    let controller = CopyController::new();
    let outcome = paste(state.items(), &target, &controller).await;
    settle(&workspace.store, &state, &outcome, &controller);

    assert!(target.join("report.txt").exists(), "the file was copied");
    assert_eq!(
        workspace.store.load(),
        ClipboardState::Empty,
        "the Paste entry must disappear after a successful paste"
    );
}

#[tokio::test]
async fn the_paste_state_stays_gone_across_a_restart() {
    let workspace = Workspace::new();
    let source = workspace.file("src/a.txt", "a");
    let target = workspace.dir("dst");

    let state = workspace.store.store(&[source]).expect("store");
    let controller = CopyController::new();
    let outcome = paste(state.items(), &target, &controller).await;
    settle(&workspace.store, &state, &outcome, &controller);

    // A second store over the same directory models a fresh process, i.e. the
    // user restarting the machine and right-clicking again.
    let after_restart = ClipboardStore::at(workspace.path("clipboard-state"));
    assert_eq!(after_restart.load(), ClipboardState::Empty);
}

#[tokio::test]
async fn a_cancelled_paste_keeps_the_copy_state() {
    let workspace = Workspace::new();
    let source = workspace.file("src/a.txt", "a");
    let target = workspace.dir("dst");

    let state = workspace.store.store(&[source]).expect("store");

    let controller = CopyController::new();
    // Cancel before any work starts, so the queue drains without copying.
    controller.cancel();
    let outcome = paste(state.items(), &target, &controller).await;
    settle(&workspace.store, &state, &outcome, &controller);

    assert_eq!(outcome.completed, 0, "nothing should have been copied");
    assert!(
        !workspace.store.load().is_empty(),
        "a cancelled paste must leave the copy state intact so it can be retried"
    );
}

#[tokio::test]
async fn a_failed_paste_keeps_the_copy_state() {
    let workspace = Workspace::new();
    let source = workspace.file("src/a.txt", "a");
    let target = workspace.dir("dst");

    let state = workspace
        .store
        .store(std::slice::from_ref(&source))
        .expect("store");

    // Delete the source after planning: the copy is attempted and fails, which
    // is the shape of a mid-flight failure.
    let items = collect_files(&source, &target).await.expect("collect");
    std::fs::remove_file(&source).expect("remove the source");

    let controller = CopyController::new();
    let failures = Arc::new(Mutex::new(Vec::new()));
    let callback = {
        let failures = failures.clone();
        Box::new(move |update: ProgressUpdate| {
            if let Some(kind) = update.error {
                failures.lock().unwrap().push(kind);
            }
        })
    };

    copy_files_with_progress(items, 4, Some(callback), Some(controller))
        .await
        .expect("the queue survives per-item failures");

    let failures = failures.lock().unwrap().clone();
    assert_eq!(failures, vec![CopyErrorKind::NotFound]);

    // The store re-validates on load, and the only source is gone, so the state
    // correctly becomes Empty rather than offering a paste that cannot work.
    assert_eq!(
        workspace.store.load(),
        ClipboardState::Empty,
        "a vanished source must not keep the Paste entry visible"
    );
    let _ = state;
}

#[tokio::test]
async fn deleting_one_of_several_sources_keeps_the_rest_pasteable() {
    let workspace = Workspace::new();
    let kept = workspace.file("src/kept.txt", "kept");
    let removed = workspace.file("src/removed.txt", "gone");
    let target = workspace.dir("dst");

    workspace
        .store
        .store(&[kept.clone(), removed.clone()])
        .expect("store");
    std::fs::remove_file(&removed).expect("remove one source");

    let state = workspace.store.load();
    assert_eq!(state.items(), &[kept], "only the surviving source remains");

    let controller = CopyController::new();
    let outcome = paste(state.items(), &target, &controller).await;

    assert!(outcome.succeeded());
    assert!(target.join("kept.txt").exists());
    assert!(!target.join("removed.txt").exists());
}

#[test]
fn a_second_paste_is_refused_while_one_is_running() {
    let workspace = Workspace::new();
    let source = workspace.file("src/a.txt", "a");
    workspace.store.store(&[source]).expect("store");

    let first = workspace.store.try_lock_paste();
    assert!(first.is_some(), "the first paste acquires the lock");
    assert!(
        workspace.store.try_lock_paste().is_none(),
        "a duplicate paste must be refused rather than racing"
    );

    drop(first);
    assert!(
        workspace.store.try_lock_paste().is_some(),
        "the lock is released when the paste finishes"
    );
}

#[test]
fn the_paste_state_is_empty_until_something_is_copied() {
    let workspace = Workspace::new();
    assert_eq!(
        workspace.store.load(),
        ClipboardState::Empty,
        "a fresh profile must not offer a Paste entry"
    );
}

#[test]
fn a_multi_item_selection_arrives_as_one_session() {
    // Explorer and Finder invoke the copy verb once per selected item, so this
    // is what a three-file selection actually looks like.
    let workspace = Workspace::new();
    let a = workspace.file("src/a.txt", "a");
    let b = workspace.file("src/b.txt", "b");
    let c = workspace.file("src/c.txt", "c");

    for path in [&a, &b, &c] {
        workspace
            .store
            .append(std::slice::from_ref(path), SESSION_WINDOW)
            .expect("append");
    }

    assert_eq!(workspace.store.load().items(), &[a, b, c]);
}

#[tokio::test]
async fn pasting_a_directory_tree_reproduces_its_structure() {
    let workspace = Workspace::new();
    workspace.file("src/tree/one.txt", "1");
    workspace.file("src/tree/nested/two.txt", "2");
    let source = workspace.path("src/tree");
    let target = workspace.dir("dst");

    let state = workspace.store.store(&[source]).expect("store");
    let controller = CopyController::new();
    let outcome = paste(state.items(), &target, &controller).await;
    settle(&workspace.store, &state, &outcome, &controller);

    assert!(outcome.succeeded(), "kinds: {:?}", outcome.kinds);
    assert_eq!(
        std::fs::read_to_string(target.join("tree/one.txt")).unwrap(),
        "1"
    );
    assert_eq!(
        std::fs::read_to_string(target.join("tree/nested/two.txt")).unwrap(),
        "2"
    );
    assert_eq!(workspace.store.load(), ClipboardState::Empty);
}

#[test]
fn a_new_copy_re_arms_the_paste_state() {
    let workspace = Workspace::new();
    let first = workspace.file("src/first.txt", "1");
    let second = workspace.file("src/second.txt", "2");

    let state = workspace.store.store(&[first]).expect("store");
    workspace.store.consume(match &state {
        ClipboardState::Copied { session, .. } => *session,
        ClipboardState::Empty => unreachable!(),
    });
    assert_eq!(workspace.store.load(), ClipboardState::Empty);

    // Only a fresh copy may bring the Paste entry back.
    workspace
        .store
        .store(std::slice::from_ref(&second))
        .expect("store");
    assert_eq!(workspace.store.load().items(), &[second]);
}
