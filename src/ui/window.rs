use super::constants::{ButtonTone, PAGE_BG, PANEL_BG, PANEL_BORDER, WINDOW_HEIGHT, WINDOW_WIDTH};
use super::progress::CopyProgress;
use super::widgets::{
    action_button, controls_row, file_info_card, header_row, message_banner, metric_chip,
    progress_bar, status_badge, truncate_middle,
};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use mcopy::CopyController;
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

    fn ensure_refresh_loop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.refresh_loop_started {
            return;
        }

        self.refresh_loop_started = true;
        let progress = self.progress.clone();

        window
            .spawn(cx, async move |cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(120))
                        .await;

                    let should_close = progress.snapshot().should_auto_close;
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
                }
            })
            .detach();
    }

    fn ensure_close_guard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_refresh_loop(window, cx);
        self.ensure_close_guard(window, cx);

        let snapshot = self.progress.snapshot();
        let accent = snapshot.accent(&self.controller);
        let pause_disabled = snapshot.is_terminal()
            || self.controller.is_cancelled()
            || (snapshot.processed_files() == 0 && snapshot.active_files == 0);
        let stop_disabled = snapshot.is_terminal() || self.controller.is_cancelled();
        let file_display = if snapshot.current_file.is_empty() {
            "Ilk dosya icin hazirlaniyor...".to_string()
        } else {
            truncate_middle(&snapshot.current_file, 54)
        };

        let pause_label = if self.controller.is_paused() {
            "Devam Et"
        } else {
            "Duraklat"
        };

        window.set_window_title(&snapshot.window_title(&self.controller));
        if snapshot.should_auto_close {
            window.remove_window();
        }

        let pause_controller = self.controller.clone();
        let pause_button = action_button(
            "pause-copy",
            pause_label,
            if self.controller.is_paused() {
                ButtonTone::Success
            } else {
                ButtonTone::Primary
            },
            pause_disabled,
            move |_, _, _| {
                if pause_controller.is_paused() {
                    pause_controller.resume();
                } else {
                    pause_controller.pause();
                }
            },
        );

        let stop_controller = self.controller.clone();
        let stop_button = action_button(
            "stop-copy",
            if self.controller.is_cancelled() {
                "Durduruluyor"
            } else {
                "Durdur"
            },
            ButtonTone::Danger,
            stop_disabled,
            move |_, _, _| stop_controller.cancel(),
        );

        div().size_full().p_4().bg(rgb(PAGE_BG)).child(
            div()
                .size_full()
                .flex()
                .flex_col()
                .overflow_hidden()
                .rounded_xl()
                .border_1()
                .border_color(rgb(PANEL_BORDER))
                .bg(rgb(PANEL_BG))
                .shadow_lg()
                .child(div().h(px(5.)).w_full().bg(rgb(accent)))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .p_4()
                        .child(header_row(
                            status_badge(snapshot.title(&self.controller), accent),
                            snapshot.title(&self.controller),
                            snapshot.subtitle(&self.controller),
                            format!("{:.0}%", snapshot.percent()),
                        ))
                        .child(progress_bar(snapshot.percent(), accent))
                        .child(controls_row(pause_button, stop_button))
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(metric_chip(
                                    "Islenen",
                                    format!(
                                        "{}/{}",
                                        snapshot.processed_files(),
                                        snapshot.total_files
                                    ),
                                    0x38bdf8,
                                ))
                                .child(metric_chip(
                                    "Aktif",
                                    snapshot.active_files.to_string(),
                                    0xfbbf24,
                                ))
                                .child(metric_chip(
                                    "Kalan",
                                    snapshot.remaining_files().to_string(),
                                    if snapshot.failed_files > 0 {
                                        0xfb7185
                                    } else {
                                        0x60a5fa
                                    },
                                )),
                        )
                        .child(file_info_card(file_display, snapshot.current_file_bytes))
                        .when(snapshot.failed_files > 0, |this| {
                            this.child(message_banner(format!(
                                "{} dosyada hata olustu. Kopyalama kalan kuyrukla devam etti.",
                                snapshot.failed_files
                            )))
                        }),
                ),
        )
    }
}

pub fn show_progress_window(progress: CopyProgress, controller: CopyController) {
    Application::new().run(move |cx| {
        let bounds = Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("mcopy".into()),
                appears_transparent: false,
                ..Default::default()
            }),
            focus: true,
            show: true,
            kind: WindowKind::PopUp,
            is_resizable: false,
            ..Default::default()
        };

        cx.open_window(options, move |_, cx| {
            let progress = progress.clone();
            let controller = controller.clone();
            cx.new(move |_| ProgressWindow::new(progress.clone(), controller.clone()))
        })
        .unwrap();

        cx.activate(true);
    });
}
