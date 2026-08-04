mod state;

use crate::platform;
use crate::ui::assets::register_fonts;
use crate::ui::shutdown::{
    self, ShutdownRequest, quit_when_last_window_closes,
};
use crate::ui::theme::Palette;
use crate::ui::widgets::logo_mark;
use gpui::*;
use state::{
    InstallOperation, InstallRenderState, OperationWorker, start_operation,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

const INSTALL_WINDOW_WIDTH: f32 = 300.0;
const INSTALL_WINDOW_HEIGHT: f32 = 240.0;
const INSTALLED_WINDOW_HEIGHT: f32 = 286.0;
const SIDE_PADDING: f32 = 24.0;
const BUTTON_WIDTH: f32 = INSTALL_WINDOW_WIDTH - (SIDE_PADDING * 2.0);
const BUTTON_HEIGHT: f32 = 39.0;

// Vertical layout offsets (absolute px from the card top). Named so a font or
// size change is a single edit instead of hunting scattered magic numbers.
const DRAG_AREA_HEIGHT: f32 = 108.0;
const STATUS_TOP: f32 = 128.0;
const INSTALL_BUTTON_TOP: f32 = 150.0;
const UNINSTALL_BUTTON_TOP: f32 = 197.0;
/// Version label position on the short (not-installed/installing) window.
const VERSION_TOP_COMPACT: f32 = 206.0;
/// Version label position on the tall (installed/uninstalling) window.
const VERSION_TOP_TALL: f32 = 252.0;

pub struct InstallWindow {
    exe_path: PathBuf,
    state: Arc<Mutex<InstallRenderState>>,
    worker: Arc<OperationWorker>,
    notify: Arc<Notify>,
    shutdown: ShutdownRequest,
    refresh_loop_started: bool,
    close_guard_registered: bool,
    appearance_observer: Option<Subscription>,
}

impl InstallWindow {
    fn new(
        exe_path: PathBuf,
        state: Arc<Mutex<InstallRenderState>>,
        worker: Arc<OperationWorker>,
        notify: Arc<Notify>,
    ) -> Self {
        Self {
            exe_path,
            state,
            worker,
            notify,
            shutdown: ShutdownRequest::new(),
            refresh_loop_started: false,
            close_guard_registered: false,
            appearance_observer: None,
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
        let notify = self.notify.clone();
        window
            .spawn(cx, async move |cx| {
                loop {
                    // Register before refreshing so a worker-thread update that
                    // lands in between still wakes the next wait.
                    let changed = notify.notified();
                    futures::pin_mut!(changed);
                    changed.as_mut().enable();

                    if cx.update(|window, _| window.refresh()).is_err() {
                        break;
                    }

                    changed.await;
                }
            })
            .detach();
    }

    fn ensure_appearance_observer(&mut self, window: &mut Window) {
        if self.appearance_observer.is_some() {
            return;
        }

        self.appearance_observer = Some(
            window.observe_window_appearance(|window, _| window.refresh()),
        );
    }

    /// Accept every close request on the first try.
    ///
    /// 0.2 returned `!is_busy()` here, which silently vetoed the OS close button
    /// for the whole duration of an install: the click did nothing, with no
    /// feedback and no way to abandon the operation. Closing now always
    /// succeeds; the worker is signalled and joined during teardown.
    fn ensure_close_guard(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.close_guard_registered {
            return;
        }

        self.close_guard_registered = true;
        let shutdown = self.shutdown.clone();
        let worker = self.worker.clone();
        window.on_window_should_close(cx, move |_, _| {
            if shutdown.begin() {
                worker.shutdown();
            }
            true
        });
    }

    /// Close the window through the one shared shutdown path.
    fn request_close(&self, window: &mut Window) {
        if self.shutdown.begin() {
            self.worker.shutdown();
        }
        shutdown::close(window);
    }
}

impl Render for InstallWindow {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.ensure_refresh_loop(window, cx);
        self.ensure_close_guard(window, cx);
        self.ensure_appearance_observer(window);

        let palette = Palette::for_appearance(window.appearance());
        let snapshot = self.state.lock().unwrap().clone();
        let visual = resolve_install_visual(&snapshot, &palette);

        window.set_window_title(visual.window_title);
        window.resize(size(px(INSTALL_WINDOW_WIDTH), px(visual.window_height)));

        let state = self.state.clone();
        let worker = self.worker.clone();
        let notify = self.notify.clone();
        let exe_path = self.exe_path.clone();
        let install_cta = install_action_button(
            "install-mcopy",
            visual.install_label,
            visual.install_disabled,
            visual.install_background,
            visual.install_hover,
            visual.install_text,
            move |_, window, _| {
                if start_operation(
                    state.clone(),
                    worker.clone(),
                    notify.clone(),
                    exe_path.clone(),
                    InstallOperation::Install,
                ) {
                    window.refresh();
                }
            },
        );

        let state = self.state.clone();
        let worker = self.worker.clone();
        let notify = self.notify.clone();
        let exe_path = self.exe_path.clone();
        let uninstall_cta = install_action_button(
            "uninstall-mcopy",
            "Uninstall",
            snapshot.is_busy(),
            palette.neutral_fill,
            palette.neutral_hover,
            palette.on_fill_text,
            move |_, window, _| {
                if start_operation(
                    state.clone(),
                    worker.clone(),
                    notify.clone(),
                    exe_path.clone(),
                    InstallOperation::Uninstall,
                ) {
                    window.refresh();
                }
            },
        );

        let close = cx.listener(|this, _: &ClickEvent, window, _| {
            this.request_close(window);
        });

        let mut card = div()
            .relative()
            .w(px(INSTALL_WINDOW_WIDTH))
            .h(px(visual.window_height))
            .bg(rgb(palette.card_bg))
            .rounded(px(12.))
            .font_family("Inter")
            .child(
                div()
                    .absolute()
                    .left(px(0.))
                    .top(px(0.))
                    .w_full()
                    .h(px(DRAG_AREA_HEIGHT))
                    .window_control_area(WindowControlArea::Drag),
            )
            .child(header(&palette))
            .child(close_button(&palette, close))
            .child(
                div()
                    .absolute()
                    .left(px(SIDE_PADDING))
                    .top(px(visual.install_button_top))
                    .child(install_cta),
            )
            .child(version_label(visual.version_top, &palette));

        if let Some(status) = visual.status_line {
            card = card.child(status_label(
                status,
                snapshot.is_error,
                visual.status_top,
                &palette,
            ));
        }

        if visual.show_uninstall {
            card = card.child(
                div()
                    .absolute()
                    .left(px(SIDE_PADDING))
                    .top(px(UNINSTALL_BUTTON_TOP))
                    .child(uninstall_cta),
            );
        }

        card
    }
}

struct InstallVisual {
    window_title: &'static str,
    window_height: f32,
    status_line: Option<String>,
    status_top: f32,
    install_label: &'static str,
    install_disabled: bool,
    install_background: u32,
    install_hover: u32,
    install_text: u32,
    install_button_top: f32,
    version_top: f32,
    show_uninstall: bool,
}

fn resolve_install_visual(
    state: &InstallRenderState,
    palette: &Palette,
) -> InstallVisual {
    let disabled = InstallVisual {
        window_title: "mcopy - Install",
        window_height: INSTALL_WINDOW_HEIGHT,
        status_line: None,
        status_top: STATUS_TOP,
        install_label: "Install",
        install_disabled: true,
        install_background: palette.install_disabled_bg,
        install_hover: palette.install_disabled_bg,
        install_text: palette.muted_text,
        install_button_top: INSTALL_BUTTON_TOP,
        version_top: VERSION_TOP_COMPACT,
        show_uninstall: false,
    };

    match state.active_operation {
        Some(InstallOperation::Install) => InstallVisual {
            window_title: "mcopy - Installing",
            status_line: Some("Installing".to_string()),
            install_label: "Installing",
            ..disabled
        },
        Some(InstallOperation::Uninstall) => InstallVisual {
            window_title: "mcopy - Uninstalling",
            window_height: INSTALLED_WINDOW_HEIGHT,
            status_line: Some("Uninstalling".to_string()),
            version_top: VERSION_TOP_TALL,
            show_uninstall: true,
            ..disabled
        },
        // A volatile location blocks installation entirely; the message says
        // what to do about it.
        None if state.is_blocked() => InstallVisual {
            window_title: "mcopy - Not Installed",
            status_line: Some(state.message.clone()),
            ..disabled
        },
        None if state.install_state.is_current_version() => InstallVisual {
            window_title: "mcopy - Already Installed",
            window_height: INSTALLED_WINDOW_HEIGHT,
            status_line: Some("Already installed".to_string()),
            version_top: VERSION_TOP_TALL,
            show_uninstall: true,
            ..disabled
        },
        None => InstallVisual {
            install_disabled: false,
            install_background: palette.success_fill,
            install_hover: palette.success_hover,
            install_text: palette.on_fill_text,
            status_line: if state.message.is_empty() {
                None
            } else {
                Some(state.message.clone())
            },
            ..disabled
        },
    }
}

fn header(palette: &Palette) -> Div {
    div()
        .child(
            div()
                .absolute()
                .left(px(24.))
                .top(px(24.))
                .w(px(27.))
                .h(px(41.))
                .child(logo_mark(27., 41., palette.logo_ink)),
        )
        .child(
            div()
                .absolute()
                .left(px(64.))
                .top(px(30.))
                .text_size(px(16.))
                .line_height(px(19.))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(palette.title_text))
                .child("mcopy"),
        )
        .child(
            div()
                .absolute()
                .left(px(64.))
                .top(px(51.))
                .text_size(px(12.))
                .line_height(px(15.))
                .text_color(rgb(palette.muted_text))
                .child("Fast and reliable file copy utility."),
        )
}

/// The close button is always live.
///
/// 0.2 disabled it while an operation ran, which combined with the vetoing
/// `on_window_should_close` handler left no way at all to dismiss the window.
fn close_button(
    palette: &Palette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id("close-install-window")
        .absolute()
        .left(px(264.))
        .top(px(12.))
        .w(px(24.))
        .h(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .text_size(px(14.))
        .line_height(px(14.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(palette.muted_text))
        .child("x")
        .hover(|this| {
            this.bg(rgb(palette.outline_hover_bg))
                .text_color(rgb(palette.title_text))
        })
        .active(|this| {
            this.bg(rgb(palette.outline_active_bg))
                .text_color(rgb(palette.title_text))
        })
        .cursor_pointer()
        .on_click(on_click)
}

fn status_label(
    label: String,
    is_error: bool,
    top: f32,
    palette: &Palette,
) -> Div {
    div()
        .absolute()
        .left(px(SIDE_PADDING))
        .top(px(top))
        .w(px(BUTTON_WIDTH))
        .text_center()
        .text_size(px(12.))
        .line_height(px(15.))
        .text_color(rgb(if is_error {
            palette.error_text
        } else {
            palette.muted_text
        }))
        .child(label)
}

fn version_label(top: f32, palette: &Palette) -> Div {
    div()
        .absolute()
        .left(px(SIDE_PADDING))
        .top(px(top))
        .w(px(BUTTON_WIDTH))
        .text_center()
        .text_size(px(11.))
        .line_height(px(14.))
        .text_color(rgb(palette.muted_text))
        .child(format!("v{}", platform::CURRENT_VERSION))
}

#[allow(clippy::too_many_arguments)]
fn install_action_button(
    id: &'static str,
    label: &'static str,
    disabled: bool,
    background: u32,
    hover_background: u32,
    text_color: u32,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let base = div()
        .id(id)
        .w(px(BUTTON_WIDTH))
        .h(px(BUTTON_HEIGHT))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .font_family("Inter")
        .text_size(px(14.))
        .line_height(px(17.))
        .font_weight(FontWeight::BOLD)
        .bg(rgb(background))
        .text_color(rgb(text_color))
        .child(label);

    if disabled {
        base.cursor_default()
    } else {
        base.hover(move |this| this.bg(rgb(hover_background)))
            .active(move |this| this.bg(rgb(hover_background)))
            .cursor_pointer()
            .on_click(on_click)
    }
}

/// Window options for the setup window.
///
/// `WindowKind::Normal` matters here for a different reason than in the progress
/// window: on macOS a `PopUp` is a non-activating `NSPanel`, so the first click
/// on an unfocused panel is consumed by activation and never reaches the close
/// button — one of the two causes of "the close button needs a second click".
pub(crate) fn install_window_options(bounds: Bounds<Pixels>) -> WindowOptions {
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
        app_id: Some(crate::APP_ID.to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        window_decorations: Some(WindowDecorations::Client),
        ..Default::default()
    }
}

pub fn show_install_window(exe_path: PathBuf) {
    let state = InstallRenderState::probe(&exe_path);
    let window_height = if state.install_state.is_current_version() {
        INSTALLED_WINDOW_HEIGHT
    } else {
        INSTALL_WINDOW_HEIGHT
    };
    let state = Arc::new(Mutex::new(state));
    let worker = Arc::new(OperationWorker::new());
    let notify = Arc::new(Notify::new());

    let worker_for_exit = worker.clone();

    Application::new().run(move |cx| {
        register_fonts(cx);
        quit_when_last_window_closes(cx);

        let bounds = Bounds::centered(
            None,
            size(px(INSTALL_WINDOW_WIDTH), px(window_height)),
            cx,
        );

        let opened = cx.open_window(install_window_options(bounds), {
            let exe_path = exe_path.clone();
            let state = state.clone();
            let worker = worker.clone();
            let notify = notify.clone();
            move |_, cx| {
                cx.new(move |_| {
                    InstallWindow::new(
                        exe_path.clone(),
                        state.clone(),
                        worker.clone(),
                        notify.clone(),
                    )
                })
            }
        });

        if let Err(error) = opened {
            crate::log_error!("could not open the setup window: {error}");
            cx.quit();
            return;
        }

        cx.activate(true);
    });

    // The event loop has returned, so no further UI work can start. Make sure
    // the worker is finished before the process exits, so an install is never
    // left half-written and no thread outlives the window.
    worker_for_exit.shutdown();
}

#[cfg(test)]
mod tests {
    use super::*;
    // `use gpui::*` above pulls in gpui's own `test` attribute macro, which
    // would shadow the built-in one and expand recursively. Bind the built-in
    // back explicitly.
    use core::prelude::v1::test;

    fn options() -> WindowOptions {
        install_window_options(Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(INSTALL_WINDOW_WIDTH), px(INSTALL_WINDOW_HEIGHT)),
        })
    }

    /// Regression guard for issue 4: a non-activating macOS panel swallows the
    /// first click, which is half of why one click sometimes did nothing.
    #[test]
    fn setup_window_is_a_normal_window() {
        assert_eq!(options().kind, WindowKind::Normal);
    }

    #[test]
    fn setup_window_appears_in_the_taskbar_with_a_title() {
        let options = options();
        assert!(options.is_minimizable);
        let title = options
            .titlebar
            .and_then(|titlebar| titlebar.title)
            .expect("a titlebar title must be set up front");
        assert_eq!(title.as_ref(), "mcopy");
    }

    #[test]
    fn setup_window_declares_an_app_id() {
        assert_eq!(options().app_id.as_deref(), Some(crate::APP_ID));
    }
}
