use crate::platform::{self, ContextMenu, ContextMenuInstallState, Platform};
use crate::{log_error, log_info};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tokio::sync::Notify;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum InstallOperation {
    Install,
    Uninstall,
}

#[derive(Clone)]
pub(crate) struct InstallRenderState {
    pub install_state: ContextMenuInstallState,
    pub active_operation: Option<InstallOperation>,
    pub message: String,
    pub is_error: bool,
    /// `None` when the executable lives somewhere durable. When set, install is
    /// blocked and the message explains how to fix it.
    pub blocked_by: Option<platform::VolatileReason>,
}

impl InstallRenderState {
    /// Build the initial state by probing the install status and the location
    /// the executable is running from.
    pub(crate) fn probe(exe_path: &std::path::Path) -> Self {
        let location = platform::location::classify(exe_path);
        let blocked_by = location.blocking_reason();

        let (install_state, mut message, mut is_error) = match Platform::state()
        {
            Ok(state) => (state, String::new(), false),
            Err(error) => {
                log_error!("could not read the install state: {error}");
                (
                    ContextMenuInstallState::NotInstalled,
                    error.to_string(),
                    true,
                )
            },
        };

        // The location problem is the more actionable of the two, so it wins
        // the single line of message space.
        if let Some(reason) = blocked_by {
            log_info!(
                "running from a volatile location: {}",
                location.exe().display()
            );
            message = reason.remedy().to_string();
            is_error = true;
        }

        Self {
            install_state,
            active_operation: None,
            message,
            is_error,
            blocked_by,
        }
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.active_operation.is_some()
    }

    pub(crate) fn is_blocked(&self) -> bool {
        self.blocked_by.is_some()
    }
}

/// Handle to the worker running an install or uninstall.
///
/// Version 0.2 detached this thread, so nothing could observe or wait for it —
/// and because the elevation helper it ran could block indefinitely, the process
/// could outlive its window. Keeping the handle lets shutdown join it.
#[derive(Default)]
pub(crate) struct OperationWorker {
    handle: Mutex<Option<JoinHandle<()>>>,
    cancelled: Arc<AtomicBool>,
}

impl OperationWorker {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Ask the worker to stop at its next checkpoint and wait briefly for it.
    ///
    /// Registry and file writes are short and must not be torn in half, so the
    /// worker is interrupted between steps rather than aborted. The join is
    /// bounded: shutdown never waits on work that is not making progress.
    pub(crate) fn shutdown(&self) {
        self.cancelled.store(true, Ordering::Release);

        let handle = self.handle.lock().unwrap().take();
        let Some(handle) = handle else {
            return;
        };

        // A completed thread joins instantly; a stuck one is abandoned rather
        // than hanging the exit, and the process teardown reclaims it.
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_millis(2_000);
        while !handle.is_finished() {
            if std::time::Instant::now() >= deadline {
                log_error!("install worker did not finish before shutdown");
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        if handle.join().is_err() {
            log_error!("install worker panicked");
        }
    }

    fn store(&self, handle: JoinHandle<()>) {
        *self.handle.lock().unwrap() = Some(handle);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// Kick off an install/uninstall on a worker thread. Returns `false` (without
/// starting anything) when the operation is not applicable right now.
pub(crate) fn start_operation(
    state: Arc<Mutex<InstallRenderState>>,
    worker: Arc<OperationWorker>,
    notify: Arc<Notify>,
    exe_path: PathBuf,
    operation: InstallOperation,
) -> bool {
    {
        let mut state = state.lock().unwrap();
        if state.is_busy() {
            return false;
        }

        if operation == InstallOperation::Install {
            if state.is_blocked() || state.install_state.is_current_version() {
                return false;
            }
        } else if !state.install_state.is_current_version() {
            return false;
        }

        state.active_operation = Some(operation);
        state.message.clear();
        state.is_error = false;
    }

    let worker_for_thread = worker.clone();
    let handle = std::thread::spawn(move || {
        let result = if worker_for_thread.is_cancelled() {
            Err(anyhow::anyhow!("cancelled"))
        } else {
            match operation {
                InstallOperation::Install => {
                    platform::install_or_update_context_menu(&exe_path)
                },
                InstallOperation::Uninstall => Platform::uninstall(),
            }
        };

        let refreshed_state = Platform::state();
        let mut state = state.lock().unwrap();

        state.active_operation = None;
        if let Ok(install_state) = refreshed_state {
            state.install_state = install_state;
        }

        match result {
            Ok(()) => {
                log_info!("{operation:?} completed");
                state.message.clear();
                state.is_error = false;
            },
            Err(error) => {
                log_error!("{operation:?} failed: {error}");
                state.message = error.to_string();
                state.is_error = true;
            },
        }
        drop(state);

        // Wake the UI loop so it repaints the finished result.
        notify.notify_waiters();
    });

    worker.store(handle);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_without_a_running_worker_returns_immediately() {
        let worker = OperationWorker::new();
        worker.shutdown();
        assert!(worker.is_cancelled());
    }

    #[test]
    fn shutdown_joins_a_finished_worker() {
        let worker = OperationWorker::new();
        worker.store(std::thread::spawn(|| {}));
        worker.shutdown();
        assert!(worker.handle.lock().unwrap().is_none());
    }

    #[test]
    fn shutdown_is_idempotent() {
        let worker = OperationWorker::new();
        worker.store(std::thread::spawn(|| {}));
        worker.shutdown();
        worker.shutdown();
    }

    /// A worker that never finishes must not stop the process from exiting.
    #[test]
    fn shutdown_gives_up_on_a_stuck_worker() {
        let worker = OperationWorker::new();
        let release = Arc::new(AtomicBool::new(false));
        let release_for_thread = release.clone();

        worker.store(std::thread::spawn(move || {
            while !release_for_thread.load(Ordering::Acquire) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }));

        let started = std::time::Instant::now();
        worker.shutdown();
        let elapsed = started.elapsed();

        // Let the thread finish so the test does not leak it.
        release.store(true, Ordering::Release);

        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "shutdown blocked for {elapsed:?}"
        );
    }
}
