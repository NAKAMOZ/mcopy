mod state;

pub use state::CopyProgress;
use state::CopyProgressSnapshot;

use crate::CopyController;
use crate::ui::assets::register_fonts;
use crate::ui::theme::{
    ACTIVE_FILL, ButtonTone, MUTED_TEXT, PAUSED_FILL, SOFT_TEXT, SUCCESS_FILL,
    TITLE_TEXT, WARNING_FILL, WINDOW_HEIGHT, WINDOW_WIDTH,
};
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
    refresh_loop_started: bool,
    close_guard_registered: bool,
}

impl ProgressWindow {
    pub fn new(progress: CopyProgress, controller: CopyController) -> Self {
        Self {
            progress,
            controller,
            refresh_loop_started: false,
            close_guard_registered: false,
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
                            window.remove_window();
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
        window.on_window_should_close(cx, move |_, _| {
            if progress.snapshot().is_terminal() {
                true
            } else {
                controller.cancel();
                false
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

        let snapshot = self.progress.snapshot();
        let pause_disabled = snapshot.is_terminal()
            || self.controller.is_cancelled()
            || (snapshot.processed_files() == 0 && snapshot.active_files == 0);
        let cancel_disabled =
            snapshot.is_terminal() || self.controller.is_cancelled();
        let visual = resolve_visual_state(&snapshot, &self.controller);
        let file_display = if snapshot.current_file.is_empty() {
            visual.file_placeholder.to_string()
        } else {
            snapshot.current_file.clone()
        };

        window.set_window_title(&snapshot.window_title(&self.controller));
        if snapshot.should_auto_close {
            window.remove_window();
        }

        let pause_controller = self.controller.clone();
        let primary_button = action_button(
            "pause-copy",
            visual.primary_label,
            visual.primary_tone,
            pause_disabled,
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
            move |_, window, _| {
                cancel_controller.cancel();
                window.refresh();
            },
        );

        let message = if snapshot.failed_files > 0 {
            format!(
                "{} items failed while the queue continued.",
                snapshot.failed_files
            )
        } else {
            String::new()
        };

        surface_card()
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
                                    .child(brand_mark())
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
                            ))
                            .child(file_name_row(file_display)),
                    ))
                    .child(message_banner(message))
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
) -> VisualState {
    if snapshot.is_terminal() {
        if controller.is_cancelled() {
            VisualState {
                status_label: "Cancelled",
                status_color: MUTED_TEXT,
                counter_primary_color: MUTED_TEXT,
                counter_secondary_color: SOFT_TEXT,
                progress_fill: WARNING_FILL,
                primary_label: "Stopped",
                primary_tone: ButtonTone::Primary,
                file_placeholder: "Copy stopped before the next item.",
            }
        } else {
            VisualState {
                status_label: "Completed",
                status_color: TITLE_TEXT,
                counter_primary_color: TITLE_TEXT,
                counter_secondary_color: MUTED_TEXT,
                progress_fill: SUCCESS_FILL,
                primary_label: "Done",
                primary_tone: ButtonTone::Primary,
                file_placeholder: "All items were copied.",
            }
        }
    } else if controller.is_cancelled() {
        VisualState {
            status_label: "Cancelling",
            status_color: MUTED_TEXT,
            counter_primary_color: MUTED_TEXT,
            counter_secondary_color: SOFT_TEXT,
            progress_fill: WARNING_FILL,
            primary_label: "Pause",
            primary_tone: ButtonTone::Primary,
            file_placeholder: "Finishing active copies before exit.",
        }
    } else if controller.is_paused() {
        VisualState {
            status_label: "Paused",
            status_color: MUTED_TEXT,
            counter_primary_color: MUTED_TEXT,
            counter_secondary_color: SOFT_TEXT,
            progress_fill: PAUSED_FILL,
            primary_label: "Resume",
            primary_tone: ButtonTone::Success,
            file_placeholder: "Waiting to resume the queue.",
        }
    } else {
        VisualState {
            status_label: "Copying Items",
            status_color: TITLE_TEXT,
            counter_primary_color: TITLE_TEXT,
            counter_secondary_color: MUTED_TEXT,
            progress_fill: ACTIVE_FILL,
            primary_label: "Pause",
            primary_tone: ButtonTone::Primary,
            file_placeholder: "Preparing the copy queue.",
        }
    }
}

pub fn show_progress_window(
    progress: CopyProgress,
    controller: CopyController,
) {
    Application::new().run(move |cx| {
        register_fonts(cx);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(
            None,
            size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)),
            cx,
        );
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: true,
            show: true,
            kind: WindowKind::PopUp,
            is_resizable: false,
            is_minimizable: false,
            // Transparent background + client-side decorations render differently
            // across Wayland compositors (some tiling WMs ignore rounding /
            // transparency). Acceptable; worth testing on GNOME/Wayland, KDE and
            // a tiling WM.
            window_background: WindowBackgroundAppearance::Transparent,
            window_decorations: Some(WindowDecorations::Client),
            ..Default::default()
        };

        cx.open_window(options, move |_, cx| {
            let progress = progress.clone();
            let controller = controller.clone();
            cx.new(move |_| {
                ProgressWindow::new(progress.clone(), controller.clone())
            })
        })
        .unwrap();

        cx.activate(true);
    });
}
