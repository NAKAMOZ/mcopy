use std::sync::{Arc, atomic::AtomicBool, atomic::Ordering};
use tokio::sync::Notify;

/// Cooperative control for copy operations.
///
/// This lets the app pause new work or cancel the remaining queue without
/// rewriting the underlying file-copy algorithm.
#[derive(Clone, Default)]
pub struct CopyController {
    paused: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
    /// Wakes tasks parked in `wait_until_resumed` on resume or cancel, so the
    /// pause path parks instead of polling.
    notify: Arc<Notify>,
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
        self.notify.notify_waiters();
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(crate) async fn wait_until_resumed(&self) -> bool {
        loop {
            if self.is_cancelled() {
                return false;
            }
            if !self.is_paused() {
                return true;
            }

            // Register interest before re-checking the flags so a resume/cancel
            // landing here cannot be missed (lost-wakeup safe), then park.
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            if !self.is_paused() || self.is_cancelled() {
                continue;
            }

            notified.await;
        }
    }
}
