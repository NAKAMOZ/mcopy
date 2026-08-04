mod state;

pub use state::CopyProgress;
use state::CopyProgressSnapshot;

use crate::CopyController;
use crate::ui::assets::register_fonts;
use crate::ui::shutdown::{
    self, ShutdownRequest, quit_when_last_window_closes,
};
use crate::ui::theme::{ButtonTone, Palette, WINDOW_HEIGHT, WINDOW_WIDTH};
use crate::ui::widgets::{
    action_button, brand_mark, controls_row, counter_display, drag_region,
    file_name_row, header_row, message_banner, progress_bar, status_text,
    surface_card,
};
use gpui::*;
use std::time::Duration;

pub struct ProgressWindow {
    progress: CopyProgress,
    controller: CopyController,
    shutdown: ShutdownRequest,
    refresh_loop_started: bool,
    close_guard_registered: bool,
    appearance_observer: Option<Subscription>,
    activated: bool,
}

impl ProgressWindow {
    pub fn new(progress: CopyProgress, controller: CopyController) -> Self {
        Self {
            progress,
            controller,
            shutdown: ShutdownRequest::new(),
            refresh_loop_started: false,
            close_guard_registered: false,
            appearance_observer: None,
            activated: false,
        }
    }

    fn ensure_refresh_loop(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.refresh_loop_started {
            return;
        }

        self.refresh_loop_started = true;
        let progress = self.progress.clone();

        window
            .spawn(cx, async move |cx| {
                loop {
                    // Register for the next state change *before* reading the
                    // snapshot so a change landing in between is not missed.
                    let changed = progress.notified();
                    futures::pin_mut!(changed);
                    changed.as_mut().enable();

                    let snapshot = progress.snapshot();
                    let should_close = snapshot.should_auto_close;
                    let updated = cx.update(|window, _| {
                        if should_close {
                            shutdown::close(window);
                        } else {
                            window.refresh();
                        }
                    });

                    if updated.is_err() || should_close {
                        break;
                    }

                    if snapshot.is_terminal() {
                        // A time-based auto-close is counting down: wake on the
                        // next change or a short timer to re-check the deadline.
                        let timer = cx
                            .background_executor()
                            .timer(Duration::from_millis(120));
                        futures::pin_mut!(timer);
                        futures::future::select(changed, timer).await;
                    } else {
                        // Otherwise repaint only when the state actually changes.
                        changed.await;
                    }
                }
            })
            .detach();
    }

    /// Repaint when the user switches the OS between light and dark while a
    /// copy is running.
    fn ensure_appearance_observer(
        &mut self,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        if self.appearance_observer.is_some() {
            return;
        }

        self.appearance_observer = Some(
            window.observe_window_appearance(|window, _| window.refresh()),
        );
    }

    /// Handle every close affordance through one path.
    ///
    /// A close request while the queue is live means "stop this copy", so it
    /// cancels the controller and lets the window stay up just long enough to
    /// show the Cancelled state before the auto-close timer removes it. That is
    /// visible feedback, unlike 0.2's silent veto. A second close request while
    /// cancellation is winding down closes immediately.
    fn ensure_close_guard(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.close_guard_registered {
            return;
        }

        self.close_guard_registered = true;

        let progress = self.progress.clone();
        let controller = self.controller.clone();
        let shutdown = self.shutdown.clone();
        window.on_window_should_close(cx, move |_, _| {
            if progress.snapshot().is_terminal() {
                return true;
            }

            // `begin` returns true only for the first request; a repeat click
            // means the user wants out now.
            if shutdown.begin() {
                controller.cancel();
                false
            } else {
                true
            }
        });
    }
}

impl Render for ProgressWindow {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.ensure_refresh_loop(window, cx);
        self.ensure_close_guard(window, cx);
        self.ensure_appearance_observer(window, cx);

        let palette = Palette::for_appearance(window.appearance());
        let snapshot = self.progress.snapshot();
        let pause_disabled = snapshot.is_terminal()
            || self.controller.is_cancelled()
            || (snapshot.processed_files() == 0 && snapshot.active_files == 0);
        let cancel_disabled =
            snapshot.is_terminal() || self.controller.is_cancelled();
        let visual =
            resolve_visual_state(&snapshot, &self.controller, &palette);
        let file_display = if snapshot.current_file.is_empty() {
            visual.file_placeholder.to_string()
        } else {
            snapshot.current_file.clone()
        };

        window.set_window_title(&snapshot.window_title(&self.controller));

        // Bring the window forward once when the job starts, then never steal
        // focus again — the user may deliberately have sent it to the back.
        if !self.activated {
            self.activated = true;
            window.activate_window();
        }

        if snapshot.should_auto_close {
            shutdown::close(window);
        }

        let pause_controller = self.controller.clone();
        let primary_button = action_button(
            "pause-copy",
            visual.primary_label,
            visual.primary_tone,
            pause_disabled,
            &palette,
            move |_, window, _| {
                if pause_controller.is_paused() {
                    pause_controller.resume();
                } else {
                    pause_controller.pause();
                }
                // Controller changes don't flow through progress.notify, so
                // repaint the toggle immediately.
                window.refresh();
            },
        );

        let cancel_controller = self.controller.clone();
        let cancel_button = action_button(
            "cancel-copy",
            "Cancel",
            ButtonTone::Outline,
            cancel_disabled,
            &palette,
            move |_, window, _| {
                cancel_controller.cancel();
                window.refresh();
            },
        );

        let failure = snapshot.failure_summary();
        let failure_is_error = snapshot.failure_is_actionable();

        surface_card(&palette)
            .w(px(WINDOW_WIDTH))
            .h(px(WINDOW_HEIGHT))
            .font_family("Inter")
            .child(
                div()
                    .w_full()
                    .h_full()
                    .flex()
                    .flex_col()
                    .justify_between()
                    .px_6()
                    .py_5()
                    .child(drag_region(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(header_row(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(brand_mark(&palette))
                                    .child(status_text(
                                        visual.status_label.to_string(),
                                        visual.status_color,
                                    )),
                                counter_display(
                                    snapshot.processed_files(),
                                    snapshot.total_files,
                                    visual.counter_primary_color,
                                    visual.counter_secondary_color,
                                ),
                            ))
                            .child(progress_bar(
                                snapshot.percent(),
                                visual.progress_fill,
                                &palette,
                            ))
                            .child(file_name_row(file_display, &palette)),
                    ))
                    .child(message_banner(
                        failure.unwrap_or_default(),
                        failure_is_error,
                        &palette,
                    ))
                    .child(controls_row(cancel_button, primary_button)),
            )
    }
}

struct VisualState {
    status_label: &'static str,
    status_color: u32,
    counter_primary_color: u32,
    counter_secondary_color: u32,
    progress_fill: u32,
    primary_label: &'static str,
    primary_tone: ButtonTone,
    file_placeholder: &'static str,
}

fn resolve_visual_state(
    snapshot: &CopyProgressSnapshot,
    controller: &CopyController,
    palette: &Palette,
) -> VisualState {
    if snapshot.is_terminal() {
        if controller.is_cancelled() {
            VisualState {
                status_label: "Cancelled",
                status_color: palette.muted_text,
                counter_primary_color: palette.muted_text,
                counter_secondary_color: palette.soft_text,
                progress_fill: palette.warning_fill,
                primary_label: "Stopped",
                primary_tone: ButtonTone::Primary,
                file_placeholder: "Copy stopped before the next item.",
            }
        } else {
            VisualState {
                status_label: "Completed",
                status_color: palette.title_text,
                counter_primary_color: palette.title_text,
                counter_secondary_color: palette.muted_text,
                progress_fill: palette.success_fill,
                primary_label: "Done",
                primary_tone: ButtonTone::Primary,
                file_placeholder: "All items were copied.",
            }
        }
    } else if controller.is_cancelled() {
        VisualState {
            status_label: "Cancelling",
            status_color: palette.muted_text,
            counter_primary_color: palette.muted_text,
            counter_secondary_color: palette.soft_text,
            progress_fill: palette.warning_fill,
            primary_label: "Pause",
            primary_tone: ButtonTone::Primary,
            file_placeholder: "Finishing active copies before exit.",
        }
    } else if controller.is_paused() {
        VisualState {
            status_label: "Paused",
            status_color: palette.muted_text,
            counter_primary_color: palette.muted_text,
            counter_secondary_color: palette.soft_text,
            progress_fill: palette.paused_fill,
            primary_label: "Resume",
            primary_tone: ButtonTone::Success,
            file_placeholder: "Waiting to resume the queue.",
        }
    } else {
        VisualState {
            status_label: "Copying Items",
            status_color: palette.title_text,
            counter_primary_color: palette.title_text,
            counter_secondary_color: palette.muted_text,
            progress_fill: palette.active_fill,
            primary_label: "Pause",
            primary_tone: ButtonTone::Primary,
            file_placeholder: "Preparing the copy queue.",
        }
    }
}

/// Window options for the copy progress window.
///
/// Extracted from the open call so the taskbar/Dock-visibility contract can be
/// asserted in a unit test without a display server.
///
/// `WindowKind::Normal` is load-bearing. gpui maps `PopUp` to `WS_EX_TOOLWINDOW`
/// on Windows (no taskbar button, no minimize box), to a non-activating
/// `NSPanel` at pop-up level on macOS (no Dock tile, no Cmd-Tab entry), and to
/// `_NET_WM_WINDOW_TYPE_NOTIFICATION` on X11 (taskbars skip it by spec). A long
/// copy therefore had no way back once it lost focus.
pub(crate) fn progress_window_options(bounds: Bounds<Pixels>) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("mcopy".into()),
            appears_transparent: true,
            ..Default::default()
        }),
        focus: true,
        show: true,
        kind: WindowKind::Normal,
        is_resizable: false,
        is_minimizable: true,
        // Lets Wayland and X11 match the window to the installed
        // `mcopy.desktop` entry, so the switcher shows the real name and icon.
        app_id: Some(crate::APP_ID.to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        window_decorations: Some(WindowDecorations::Client),
        ..Default::default()
    }
}

pub fn show_progress_window(
    progress: CopyProgress,
    controller: CopyController,
) {
    Application::new().run(move |cx| {
        register_fonts(cx);
        quit_when_last_window_closes(cx);

        let bounds = Bounds::centered(
            None,
            size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)),
            cx,
        );

        let opened = cx.open_window(progress_window_options(bounds), {
            let progress = progress.clone();
            let controller = controller.clone();
            move |_, cx| {
                cx.new(move |_| {
                    ProgressWindow::new(progress.clone(), controller.clone())
                })
            }
        });

        if let Err(error) = opened {
            // Without a window there is nothing to drive the copy to a visible
            // conclusion, so cancel rather than copying invisibly.
            crate::log_error!("could not open the progress window: {error}");
            controller.cancel();
            cx.quit();
            return;
        }

        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::theme::AUTO_CLOSE_DELAY;
    // `use gpui::*` above pulls in gpui's own `test` attribute macro, which
    // would shadow the built-in one and expand recursively.
    use core::prelude::v1::test;

    fn options() -> WindowOptions {
        progress_window_options(Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)),
        })
    }

    /// Regression guard for issue 2: a copy that outlives the user's attention
    /// must remain reachable from the taskbar, Dock, or window switcher.
    #[test]
    fn progress_window_is_a_normal_window() {
        assert_eq!(options().kind, WindowKind::Normal);
    }

    #[test]
    fn progress_window_can_be_minimized_and_restored() {
        assert!(options().is_minimizable);
    }

    #[test]
    fn progress_window_declares_an_app_id_for_linux_taskbars() {
        assert_eq!(options().app_id.as_deref(), Some(crate::APP_ID));
    }

    /// The taskbar label is read from the window title at map time, so it must
    /// be set before the first paint rather than only in `render`.
    #[test]
    fn progress_window_has_a_title_before_first_paint() {
        let title = options()
            .titlebar
            .and_then(|titlebar| titlebar.title)
            .expect("a titlebar title must be set up front");
        assert_eq!(title.as_ref(), "mcopy");
    }

    #[test]
    fn auto_close_delay_is_short_enough_to_feel_automatic() {
        assert!(AUTO_CLOSE_DELAY <= Duration::from_secs(2));
    }
}
