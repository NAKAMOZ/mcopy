use super::constants::AUTO_CLOSE_DELAY;
use mcopy::{CopyController, ProgressPhase, ProgressUpdate};
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum TerminalState {
    Completed,
    Cancelled,
}

struct CopyProgressState {
    current_file: String,
    completed_files: usize,
    failed_files: usize,
    active_files: usize,
    total_files: usize,
    terminal_state: Option<TerminalState>,
    terminal_since: Option<Instant>,
}

#[derive(Clone)]
pub struct CopyProgress {
    state: Arc<Mutex<CopyProgressState>>,
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
            state: Arc::new(Mutex::new(CopyProgressState {
                current_file: String::new(),
                completed_files: 0,
                failed_files: 0,
                active_files: 0,
                total_files,
                terminal_state: None,
                terminal_since: None,
            })),
        }
    }

    pub fn apply(&self, update: ProgressUpdate) {
        let mut state = self.state.lock().unwrap();

        state.current_file = update.file_name;

        match update.phase {
            ProgressPhase::Started => {
                if state.terminal_state.is_none() {
                    state.active_files += 1;
                }
            }
            ProgressPhase::Finished => {
                state.active_files = state.active_files.saturating_sub(1);
                state.completed_files += 1;
            }
            ProgressPhase::Failed => {
                state.active_files = state.active_files.saturating_sub(1);
                state.failed_files += 1;
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
        let state = self.state.lock().unwrap();

        CopyProgressSnapshot {
            current_file: state.current_file.clone(),
            completed_files: state.completed_files,
            failed_files: state.failed_files,
            active_files: state.active_files,
            total_files: state.total_files,
            terminal_state: state.terminal_state,
            should_auto_close: state
                .terminal_since
                .map(|instant| instant.elapsed() >= AUTO_CLOSE_DELAY)
                .unwrap_or(false),
        }
    }

    fn mark_terminal(&self, terminal_state: TerminalState) {
        let mut state = self.state.lock().unwrap();
        state.active_files = 0;
        state.terminal_state = Some(terminal_state);
        if state.terminal_since.is_none() {
            state.terminal_since = Some(Instant::now());
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
