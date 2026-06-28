use std::sync::{Arc, atomic::AtomicBool, atomic::Ordering};
use std::time::Duration;

/// Cooperative control for copy operations.
///
/// This lets the app pause new work or cancel the remaining queue without
/// rewriting the underlying file-copy algorithm.
#[derive(Clone, Default)]
pub struct CopyController {
    paused: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
}

impl CopyController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pause(&self) {
        if !self.is_cancelled() {
            self.paused.store(true, Ordering::SeqCst);
        }
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub(crate) async fn wait_until_resumed(&self) -> bool {
        while self.is_paused() {
            if self.is_cancelled() {
                return false;
            }

            tokio::time::sleep(Duration::from_millis(80)).await;
        }

        !self.is_cancelled()
    }
}
