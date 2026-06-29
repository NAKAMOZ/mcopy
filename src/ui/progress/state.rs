use crate::ui::theme::AUTO_CLOSE_DELAY;
use crate::{CopyController, ProgressPhase, ProgressUpdate};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

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
}

struct CopyProgressInner {
    completed_files: AtomicUsize,
    failed_files: AtomicUsize,
    active_files: AtomicUsize,
    total_files: usize,
    shared: Mutex<CopyProgressShared>,
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
                }),
            }),
        }
    }

    pub fn apply(&self, update: ProgressUpdate) {
        // Lock only to store the filename and read the terminal flag; the
        // counters are bumped lock-free.
        let is_terminal = {
            let mut shared = self.inner.shared.lock().unwrap();
            shared.current_file = update.file_name;
            shared.terminal_state.is_some()
        };

        match update.phase {
            ProgressPhase::Started => {
                if !is_terminal {
                    self.inner.active_files.fetch_add(1, Ordering::Relaxed);
                }
            }
            ProgressPhase::Finished => {
                saturating_dec(&self.inner.active_files);
                self.inner.completed_files.fetch_add(1, Ordering::Relaxed);
            }
            ProgressPhase::Failed => {
                saturating_dec(&self.inner.active_files);
                self.inner.failed_files.fetch_add(1, Ordering::Relaxed);
            }
        }
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
        let completed_files = self.inner.completed_files.load(Ordering::Relaxed);
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
            should_auto_close: shared
                .terminal_since
                .map(|instant| instant.elapsed() >= AUTO_CLOSE_DELAY)
                .unwrap_or(false),
        }
    }

    fn mark_terminal(&self, terminal_state: TerminalState) {
        self.inner.active_files.store(0, Ordering::Relaxed);
        let mut shared = self.inner.shared.lock().unwrap();
        shared.terminal_state = Some(terminal_state);
        if shared.terminal_since.is_none() {
            shared.terminal_since = Some(Instant::now());
        }
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

    pub fn window_title(&self, controller: &CopyController) -> String {
        match self.terminal_state {
            Some(TerminalState::Completed) => "mcopy - Completed".to_string(),
            Some(TerminalState::Cancelled) => "mcopy - Cancelled".to_string(),
            None if controller.is_cancelled() => "mcopy - Cancelling".to_string(),
            None if controller.is_paused() => "mcopy - Paused".to_string(),
            None if self.processed_files() == 0 && self.active_files == 0 => {
                "mcopy - Preparing".to_string()
            }
            None => "mcopy - Copying".to_string(),
        }
    }
}
