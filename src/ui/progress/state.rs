use crate::copy::CopyErrorKind;
use crate::ui::theme::AUTO_CLOSE_DELAY;
use crate::{CopyController, ProgressPhase, ProgressUpdate};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};
use tokio::sync::{Notify, futures::Notified};

#[derive(Clone, Copy, PartialEq, Eq)]
enum TerminalState {
    Completed,
    Cancelled,
}

/// Fields that genuinely need exclusive access (the current filename string and
/// the terminal markers); the counters live in lock-free atomics alongside.
struct CopyProgressShared {
    current_file: String,
    terminal_state: Option<TerminalState>,
    terminal_since: Option<Instant>,
    /// Most specific failure cause seen so far, so the banner can name a reason
    /// instead of reporting an anonymous failure count.
    dominant_error: Option<CopyErrorKind>,
}

struct CopyProgressInner {
    completed_files: AtomicUsize,
    failed_files: AtomicUsize,
    active_files: AtomicUsize,
    total_files: usize,
    shared: Mutex<CopyProgressShared>,
    /// Wakes the UI refresh loop when the state actually changes, replacing the
    /// blind fixed-interval repaint.
    notify: Notify,
}

#[derive(Clone)]
pub struct CopyProgress {
    inner: Arc<CopyProgressInner>,
}

/// Saturating decrement for an `AtomicUsize` (never wraps below zero).
fn saturating_dec(counter: &AtomicUsize) {
    let mut current = counter.load(Ordering::Relaxed);
    while current > 0 {
        match counter.compare_exchange_weak(
            current,
            current - 1,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(observed) => current = observed,
        }
    }
}

#[derive(Clone)]
pub(crate) struct CopyProgressSnapshot {
    pub current_file: String,
    pub completed_files: usize,
    pub failed_files: usize,
    pub active_files: usize,
    pub total_files: usize,
    terminal_state: Option<TerminalState>,
    pub should_auto_close: bool,
    pub dominant_error: Option<CopyErrorKind>,
}

impl CopyProgress {
    pub fn new(total_files: usize) -> Self {
        Self {
            inner: Arc::new(CopyProgressInner {
                completed_files: AtomicUsize::new(0),
                failed_files: AtomicUsize::new(0),
                active_files: AtomicUsize::new(0),
                total_files,
                shared: Mutex::new(CopyProgressShared {
                    current_file: String::new(),
                    terminal_state: None,
                    terminal_since: None,
                    dominant_error: None,
                }),
                notify: Notify::new(),
            }),
        }
    }

    /// Future that resolves the next time the state changes. The UI awaits this
    /// instead of polling on a timer.
    pub(crate) fn notified(&self) -> Notified<'_> {
        self.inner.notify.notified()
    }

    pub fn apply(&self, update: ProgressUpdate) {
        // Lock only to store the filename, fold in any failure cause, and read
        // the terminal flag; the counters are bumped lock-free.
        let is_terminal = {
            let mut shared = self.inner.shared.lock().unwrap();
            shared.current_file = update.file_name;
            if let Some(kind) = update.error {
                // Keep the most specific cause; see `CopyErrorKind::dominant`.
                shared.dominant_error = shared
                    .dominant_error
                    .map_or(Some(kind), |seen| Some(seen.max(kind)));
            }
            shared.terminal_state.is_some()
        };

        match update.phase {
            ProgressPhase::Started => {
                if !is_terminal {
                    self.inner.active_files.fetch_add(1, Ordering::Relaxed);
                }
            },
            ProgressPhase::Finished => {
                saturating_dec(&self.inner.active_files);
                self.inner.completed_files.fetch_add(1, Ordering::Relaxed);
            },
            ProgressPhase::Failed => {
                saturating_dec(&self.inner.active_files);
                self.inner.failed_files.fetch_add(1, Ordering::Relaxed);
            },
        }

        self.inner.notify.notify_waiters();
    }

    /// Number of items that failed.
    ///
    /// The paste flow reads this to decide whether the copy state may be
    /// consumed: a run with failures is not a success, so the user keeps what
    /// they copied and can retry.
    pub fn failed_count(&self) -> usize {
        self.inner.failed_files.load(Ordering::Relaxed)
    }

    pub fn complete(&self) {
        self.mark_terminal(TerminalState::Completed);
    }

    pub fn cancelled(&self) {
        self.mark_terminal(TerminalState::Cancelled);
    }

    pub(crate) fn snapshot(&self) -> CopyProgressSnapshot {
        // Read the counters lock-free, then take the lock only for the filename
        // and terminal markers.
        let completed_files =
            self.inner.completed_files.load(Ordering::Relaxed);
        let failed_files = self.inner.failed_files.load(Ordering::Relaxed);
        let active_files = self.inner.active_files.load(Ordering::Relaxed);

        let shared = self.inner.shared.lock().unwrap();
        CopyProgressSnapshot {
            current_file: shared.current_file.clone(),
            completed_files,
            failed_files,
            active_files,
            total_files: self.inner.total_files,
            terminal_state: shared.terminal_state,
            // Auto-close is a convenience for the clean case only. When items
            // failed, the window stays up so the reason is actually readable —
            // dismissing an error banner after 900ms would reintroduce exactly
            // the silent failure this release is meant to remove.
            should_auto_close: failed_files == 0
                && shared.terminal_since.is_some_and(|instant| {
                    instant.elapsed() >= AUTO_CLOSE_DELAY
                }),
            dominant_error: shared.dominant_error,
        }
    }

    /// Latch the outcome. First writer wins.
    ///
    /// A terminal state is terminal: once a run has been marked cancelled, a
    /// late `complete()` must not relabel it a success. `terminal_since`
    /// already behaved this way; the state itself did not, which left the
    /// window title and the auto-close timer able to disagree about what
    /// happened.
    fn mark_terminal(&self, terminal_state: TerminalState) {
        self.inner.active_files.store(0, Ordering::Relaxed);
        {
            let mut shared = self.inner.shared.lock().unwrap();
            if shared.terminal_state.is_some() {
                return;
            }
            shared.terminal_state = Some(terminal_state);
            shared.terminal_since = Some(Instant::now());
        }

        self.inner.notify.notify_waiters();
    }
}

impl CopyProgressSnapshot {
    pub fn processed_files(&self) -> usize {
        (self.completed_files + self.failed_files).min(self.total_files)
    }

    pub fn percent(&self) -> f32 {
        if self.total_files == 0 {
            return 0.0;
        }

        (self.processed_files() as f32 / self.total_files as f32) * 100.0
    }

    pub fn is_terminal(&self) -> bool {
        self.terminal_state.is_some()
    }

    /// One line naming what went wrong, or `None` when nothing did.
    ///
    /// Replaces 0.2's bare "N items failed", which told the user that something
    /// was wrong but never what or what to do about it.
    pub fn failure_summary(&self) -> Option<String> {
        if self.failed_files == 0 {
            return None;
        }

        let noun = if self.failed_files == 1 {
            "item"
        } else {
            "items"
        };
        let kind = self.dominant_error.unwrap_or(CopyErrorKind::Other);
        let mut summary = format!(
            "{} {noun} skipped: {}",
            self.failed_files,
            kind.describe()
        );

        if let Some(hint) = kind.hint() {
            summary.push(' ');
            summary.push_str(hint);
        }

        Some(summary)
    }

    /// Whether the failure banner should be styled as an error.
    pub fn failure_is_actionable(&self) -> bool {
        self.failed_files > 0
            && self
                .dominant_error
                .is_some_and(CopyErrorKind::is_actionable)
    }

    pub fn window_title(&self, controller: &CopyController) -> String {
        match self.terminal_state {
            Some(TerminalState::Completed) => "mcopy - Completed".to_string(),
            Some(TerminalState::Cancelled) => "mcopy - Cancelled".to_string(),
            None if controller.is_cancelled() => {
                "mcopy - Cancelling".to_string()
            },
            None if controller.is_paused() => "mcopy - Paused".to_string(),
            None if self.processed_files() == 0 && self.active_files == 0 => {
                "mcopy - Preparing".to_string()
            },
            None => "mcopy - Copying".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn update(
        phase: ProgressPhase,
        error: Option<CopyErrorKind>,
    ) -> ProgressUpdate {
        ProgressUpdate {
            phase,
            processed_files: 0,
            file_name: "item.txt".to_string(),
            file_bytes: 0,
            error,
        }
    }

    #[test]
    fn counts_successes_and_failures_separately() {
        let progress = CopyProgress::new(3);
        progress.apply(update(ProgressPhase::Finished, None));
        progress.apply(update(
            ProgressPhase::Failed,
            Some(CopyErrorKind::NotFound),
        ));

        let snapshot = progress.snapshot();
        assert_eq!(snapshot.completed_files, 1);
        assert_eq!(snapshot.failed_files, 1);
        assert_eq!(snapshot.processed_files(), 2);
    }

    #[test]
    fn active_count_never_goes_negative() {
        let progress = CopyProgress::new(1);
        // A Finished without a matching Started would underflow a naive counter.
        progress.apply(update(ProgressPhase::Finished, None));
        assert_eq!(progress.snapshot().active_files, 0);
    }

    #[test]
    fn percent_is_zero_for_an_empty_queue() {
        assert_eq!(CopyProgress::new(0).snapshot().percent(), 0.0);
    }

    #[test]
    fn percent_reaches_one_hundred_when_every_item_is_processed() {
        let progress = CopyProgress::new(2);
        progress.apply(update(ProgressPhase::Finished, None));
        progress.apply(update(ProgressPhase::Finished, None));
        assert_eq!(progress.snapshot().percent(), 100.0);
    }

    #[test]
    fn no_summary_when_nothing_failed() {
        let progress = CopyProgress::new(1);
        progress.apply(update(ProgressPhase::Finished, None));
        assert_eq!(progress.snapshot().failure_summary(), None);
    }

    #[test]
    fn summary_names_the_dominant_cause() {
        let progress = CopyProgress::new(2);
        progress.apply(update(
            ProgressPhase::Failed,
            Some(CopyErrorKind::NotFound),
        ));
        progress.apply(update(
            ProgressPhase::Failed,
            Some(CopyErrorKind::PermissionDenied),
        ));

        let snapshot = progress.snapshot();
        let summary =
            snapshot.failure_summary().expect("failures were recorded");
        assert!(summary.starts_with("2 items skipped: permission denied"));
        // The actionable next step must ride along with the cause.
        assert!(summary.len() > "2 items skipped: permission denied".len());
        assert!(snapshot.failure_is_actionable());
    }

    #[test]
    fn summary_is_singular_for_one_failure() {
        let progress = CopyProgress::new(1);
        progress
            .apply(update(ProgressPhase::Failed, Some(CopyErrorKind::NoSpace)));
        assert!(
            progress
                .snapshot()
                .failure_summary()
                .unwrap()
                .starts_with("1 item skipped:")
        );
    }

    #[test]
    fn a_clean_run_is_eligible_for_auto_close() {
        let progress = CopyProgress::new(1);
        progress.apply(update(ProgressPhase::Finished, None));
        progress.complete();

        let snapshot = progress.snapshot();
        assert!(snapshot.is_terminal());
        // The delay has not elapsed yet, but nothing blocks it either.
        assert_eq!(snapshot.failed_files, 0);
    }

    /// Regression guard: an error banner that auto-dismisses after 900ms is a
    /// silent failure with extra steps.
    #[test]
    fn a_run_with_failures_never_auto_closes() {
        let progress = CopyProgress::new(1);
        progress.apply(update(
            ProgressPhase::Failed,
            Some(CopyErrorKind::PermissionDenied),
        ));
        progress.complete();

        std::thread::sleep(AUTO_CLOSE_DELAY + Duration::from_millis(50));

        let snapshot = progress.snapshot();
        assert!(snapshot.is_terminal());
        assert!(
            !snapshot.should_auto_close,
            "the window must stay up so the failure reason stays readable"
        );
    }

    #[test]
    fn terminal_state_is_latched_on_first_transition() {
        let progress = CopyProgress::new(1);
        progress.cancelled();
        progress.complete();

        // Cancellation won the race, so the window must not claim success.
        let controller = CopyController::new();
        assert_eq!(
            progress.snapshot().window_title(&controller),
            "mcopy - Cancelled"
        );
    }

    #[test]
    fn window_title_tracks_controller_state() {
        let progress = CopyProgress::new(4);
        let controller = CopyController::new();

        assert_eq!(
            progress.snapshot().window_title(&controller),
            "mcopy - Preparing"
        );

        progress.apply(update(ProgressPhase::Started, None));
        assert_eq!(
            progress.snapshot().window_title(&controller),
            "mcopy - Copying"
        );

        controller.pause();
        assert_eq!(
            progress.snapshot().window_title(&controller),
            "mcopy - Paused"
        );

        controller.cancel();
        assert_eq!(
            progress.snapshot().window_title(&controller),
            "mcopy - Cancelling"
        );
    }
}
