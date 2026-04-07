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
    current_file_bytes: u64,
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
    pub current_file_bytes: u64,
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
                current_file_bytes: 0,
                terminal_state: None,
                terminal_since: None,
            })),
        }
    }

    pub fn apply(&self, update: ProgressUpdate) {
        let mut state = self.state.lock().unwrap();

        state.current_file = update.file_name;
        state.current_file_bytes = update.file_bytes;

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
            current_file_bytes: state.current_file_bytes,
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

    pub fn remaining_files(&self) -> usize {
        self.total_files
            .saturating_sub(self.processed_files() + self.active_files)
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

    pub fn title(&self, controller: &CopyController) -> String {
        match self.terminal_state {
            Some(TerminalState::Completed) => "Kopyalama tamamlandi".to_string(),
            Some(TerminalState::Cancelled) => "Kopyalama durduruldu".to_string(),
            None if controller.is_cancelled() => {
                if self.active_files > 0 {
                    "Durduruluyor".to_string()
                } else {
                    "Durdurma istegi alindi".to_string()
                }
            }
            None if controller.is_paused() => {
                if self.active_files > 0 {
                    "Duraklatiliyor".to_string()
                } else {
                    "Duraklatildi".to_string()
                }
            }
            None if self.processed_files() == 0 && self.active_files == 0 => {
                "Hazirlaniyor".to_string()
            }
            None => "Kopyalaniyor".to_string(),
        }
    }

    pub fn subtitle(&self, controller: &CopyController) -> String {
        match self.terminal_state {
            Some(TerminalState::Completed) => {
                if self.failed_files > 0 {
                    format!(
                        "{} dosya kopyalandi, {} dosyada hata olustu",
                        self.completed_files, self.failed_files
                    )
                } else {
                    format!("{} dosya basariyla kopyalandi", self.completed_files)
                }
            }
            Some(TerminalState::Cancelled) => format!(
                "{} dosya islendi, {} dosya sirada kaldi",
                self.processed_files(),
                self.remaining_files()
            ),
            None if controller.is_cancelled() => {
                if self.active_files > 0 {
                    format!(
                        "{} aktif is tamamlaninca kalan kuyruk duracak",
                        self.active_files
                    )
                } else {
                    "Yeni is baslatilmiyor".to_string()
                }
            }
            None if controller.is_paused() => {
                if self.active_files > 0 {
                    format!(
                        "{} aktif is guvenli sekilde tamamlaniyor",
                        self.active_files
                    )
                } else {
                    format!("{} dosya beklemede", self.remaining_files())
                }
            }
            None if self.processed_files() == 0 && self.active_files == 0 => {
                "Klasor yapisi ve kuyruk hazirlaniyor".to_string()
            }
            None if self.active_files > 0 => format!(
                "{} aktif is, {} dosya sirada",
                self.active_files,
                self.remaining_files()
            ),
            None => format!("{} dosya islendi", self.processed_files()),
        }
    }

    pub fn accent(&self, controller: &CopyController) -> u32 {
        match self.terminal_state {
            Some(TerminalState::Completed) => 0x34d399,
            Some(TerminalState::Cancelled) => 0xfb923c,
            None if controller.is_cancelled() => 0xf97316,
            None if controller.is_paused() => 0xfbbf24,
            None if self.processed_files() == 0 && self.active_files == 0 => 0x60a5fa,
            None => 0x38bdf8,
        }
    }

    pub fn window_title(&self, controller: &CopyController) -> String {
        match self.terminal_state {
            Some(TerminalState::Completed) => "mcopy - Tamamlandi".to_string(),
            Some(TerminalState::Cancelled) => "mcopy - Durduruldu".to_string(),
            None if controller.is_cancelled() => "mcopy - Durduruluyor".to_string(),
            None if controller.is_paused() => "mcopy - Duraklatildi".to_string(),
            None if self.processed_files() == 0 && self.active_files == 0 => {
                "mcopy - Hazirlaniyor".to_string()
            }
            None => "mcopy - Kopyalaniyor".to_string(),
        }
    }
}
