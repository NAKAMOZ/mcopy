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
            self.paused.store(true, Ordering::Release);
        }
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::Release);
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
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
