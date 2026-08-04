//! One shutdown path, shared by both windows.
//!
//! Version 0.2 had several: the custom close button called `cx.quit()`, the OS
//! close button went through `on_window_should_close`, and the install window
//! registered no `on_window_closed` handler at all. Those paths disagreed about
//! what "closed" meant, which is why a single click sometimes did nothing.
//!
//! Everything now funnels through [`ShutdownRequest`]:
//!
//! ```text
//! close request (custom x / OS close / Cmd-W / Alt-F4)
//!   -> ShutdownRequest::begin()   idempotent; a second click is a no-op
//!   -> cancel the in-flight operation
//!   -> window.remove_window()
//!   -> on_window_closed: last window gone -> cx.quit()
//!   -> Application::run() returns -> workers joined -> process exits
//! ```
//!
//! `cx.quit()` is deliberately *not* called from a click handler. On Windows it
//! is `PostQuitMessage`, which sets the quit flag for the calling thread's
//! message queue without destroying the window; racing that against pending
//! input is what produced the "needs a second click" behavior.

use gpui::{App, Window};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A latch marking that shutdown has been requested.
///
/// Cloneable and cheap; every close affordance shares one instance so repeated
/// or concurrent requests collapse into a single teardown.
#[derive(Clone, Default)]
pub struct ShutdownRequest {
    begun: Arc<AtomicBool>,
}

impl ShutdownRequest {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark shutdown as requested.
    ///
    /// Returns `true` only for the caller that won the race, so teardown work
    /// runs exactly once no matter how many times the user clicks.
    pub fn begin(&self) -> bool {
        !self.begun.swap(true, Ordering::AcqRel)
    }
}

/// Quit once the last window goes away.
///
/// Both windows register this, so closing the final window always terminates
/// the process instead of leaving an invisible event loop running.
pub fn quit_when_last_window_closes(cx: &mut App) {
    cx.on_window_closed(|cx| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
}

/// Close this window through the normal platform teardown.
///
/// Paired with [`quit_when_last_window_closes`], this is the only close call
/// the UI needs: the window is destroyed, and the app exits when none remain.
pub fn close(window: &mut Window) {
    window.remove_window();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::thread;

    #[test]
    fn begin_returns_true_only_once() {
        let request = ShutdownRequest::new();
        assert!(request.begin(), "first request should win");
        assert!(!request.begin(), "second click must be a no-op");
        assert!(!request.begin());
    }

    #[test]
    fn clones_share_one_latch() {
        let request = ShutdownRequest::new();
        let clone = request.clone();

        assert!(request.begin());
        assert!(
            !clone.begin(),
            "a clone must observe the original's shutdown"
        );
    }

    /// The close button, the OS close handler and a keyboard shortcut can all
    /// fire nearly simultaneously; exactly one must drive teardown.
    #[test]
    fn concurrent_requests_elect_a_single_winner() {
        let request = ShutdownRequest::new();
        let winners = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..16)
            .map(|_| {
                let request = request.clone();
                let winners = winners.clone();
                thread::spawn(move || {
                    if request.begin() {
                        winners.fetch_add(1, Ordering::Relaxed);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("worker thread panicked");
        }

        assert_eq!(winners.load(Ordering::Relaxed), 1);
    }
}
