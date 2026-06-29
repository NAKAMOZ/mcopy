mod state;

use crate::platform;
use crate::ui::assets::register_fonts;
use crate::ui::theme::{
    BLACK_FILL, BLACK_HOVER, CARD_BG, ERROR_TEXT, INSTALL_DISABLED_BG,
    MUTED_TEXT, SUCCESS_FILL, SUCCESS_HOVER, TITLE_TEXT,
};
use crate::ui::widgets::logo_mark;
use gpui::*;
use state::{InstallOperation, InstallRenderState, start_operation};
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
    notify: Arc<Notify>,
    refresh_loop_started: bool,
    close_guard_registered: bool,
}

impl InstallWindow {
    fn new(
        exe_path: PathBuf,
        state: Arc<Mutex<InstallRenderState>>,
        notify: Arc<Notify>,
    ) -> Self {
        Self {
            exe_path,
            state,
            notify,
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

    fn ensure_close_guard(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.close_guard_registered {
            return;
        }

        self.close_guard_registered = true;
        let state = self.state.clone();
        window.on_window_should_close(cx, move |_, _| {
            !state.lock().unwrap().is_busy()
        });
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

        let snapshot = self.state.lock().unwrap().clone();
        let visual = resolve_install_visual(&snapshot);

        window.set_window_title(visual.window_title);
        window.resize(size(px(INSTALL_WINDOW_WIDTH), px(visual.window_height)));

        let state = self.state.clone();
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
                    notify.clone(),
                    exe_path.clone(),
                    InstallOperation::Install,
                ) {
                    window.refresh();
                }
            },
        );

        let state = self.state.clone();
        let notify = self.notify.clone();
        let exe_path = self.exe_path.clone();
        let uninstall_cta = install_action_button(
            "uninstall-mcopy",
            "Uninstall",
            snapshot.is_busy(),
            BLACK_FILL,
            BLACK_HOVER,
            CARD_BG,
            move |_, window, _| {
                if start_operation(
                    state.clone(),
                    notify.clone(),
                    exe_path.clone(),
                    InstallOperation::Uninstall,
                ) {
                    window.refresh();
                }
            },
        );

        let mut card = div()
            .relative()
            .w(px(INSTALL_WINDOW_WIDTH))
            .h(px(visual.window_height))
            .bg(rgb(CARD_BG))
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
            .child(header())
            .child(close_button(snapshot.is_busy()))
            .child(
                div()
                    .absolute()
                    .left(px(SIDE_PADDING))
                    .top(px(visual.install_button_top))
                    .child(install_cta),
            )
            .child(version_label(visual.version_top));

        if let Some(status) = visual.status_line {
            card = card.child(status_label(
                status,
                snapshot.is_error,
                visual.status_top,
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

fn resolve_install_visual(state: &InstallRenderState) -> InstallVisual {
    match state.active_operation {
        Some(InstallOperation::Install) => InstallVisual {
            window_title: "mcopy - Installing",
            window_height: INSTALL_WINDOW_HEIGHT,
            status_line: Some("Installing".to_string()),
            status_top: STATUS_TOP,
            install_label: "Installing",
            install_disabled: true,
            install_background: INSTALL_DISABLED_BG,
            install_hover: INSTALL_DISABLED_BG,
            install_text: MUTED_TEXT,
            install_button_top: INSTALL_BUTTON_TOP,
            version_top: VERSION_TOP_COMPACT,
            show_uninstall: false,
        },
        Some(InstallOperation::Uninstall) => InstallVisual {
            window_title: "mcopy - Uninstalling",
            window_height: INSTALLED_WINDOW_HEIGHT,
            status_line: Some("Uninstalling".to_string()),
            status_top: STATUS_TOP,
            install_label: "Install",
            install_disabled: true,
            install_background: INSTALL_DISABLED_BG,
            install_hover: INSTALL_DISABLED_BG,
            install_text: MUTED_TEXT,
            install_button_top: INSTALL_BUTTON_TOP,
            version_top: VERSION_TOP_TALL,
            show_uninstall: true,
        },
        None if state.install_state.is_current_version() => InstallVisual {
            window_title: "mcopy - Already Installed",
            window_height: INSTALLED_WINDOW_HEIGHT,
            status_line: Some("Already installed".to_string()),
            status_top: STATUS_TOP,
            install_label: "Install",
            install_disabled: true,
            install_background: INSTALL_DISABLED_BG,
            install_hover: INSTALL_DISABLED_BG,
            install_text: MUTED_TEXT,
            install_button_top: INSTALL_BUTTON_TOP,
            version_top: VERSION_TOP_TALL,
            show_uninstall: true,
        },
        None => InstallVisual {
            window_title: "mcopy - Install",
            window_height: INSTALL_WINDOW_HEIGHT,
            status_line: if state.message.is_empty() {
                None
            } else {
                Some(state.message.clone())
            },
            status_top: STATUS_TOP,
            install_label: "Install",
            install_disabled: false,
            install_background: SUCCESS_FILL,
            install_hover: SUCCESS_HOVER,
            install_text: CARD_BG,
            install_button_top: INSTALL_BUTTON_TOP,
            version_top: VERSION_TOP_COMPACT,
            show_uninstall: false,
        },
    }
}

fn header() -> Div {
    div()
        .child(
            div()
                .absolute()
                .left(px(24.))
                .top(px(24.))
                .w(px(27.))
                .h(px(41.))
                .child(logo_mark(27., 41.)),
        )
        .child(
            div()
                .absolute()
                .left(px(64.))
                .top(px(30.))
                .text_size(px(16.))
                .line_height(px(19.))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(TITLE_TEXT))
                .child("mcopy"),
        )
        .child(
            div()
                .absolute()
                .left(px(64.))
                .top(px(51.))
                .text_size(px(12.))
                .line_height(px(15.))
                .text_color(rgb(MUTED_TEXT))
                .child("Fast and reliable file copy utility."),
        )
}

fn close_button(disabled: bool) -> impl IntoElement {
    let base = div()
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
        .text_color(rgb(MUTED_TEXT))
        .child("x");

    if disabled {
        base.cursor_default()
    } else {
        base.hover(|this| this.bg(rgb(0xf5f5f5)).text_color(rgb(TITLE_TEXT)))
            .active(|this| this.bg(rgb(0xeeeeee)).text_color(rgb(TITLE_TEXT)))
            .cursor_pointer()
            .on_click(|_, _, cx| cx.quit())
    }
}

fn status_label(label: String, is_error: bool, top: f32) -> Div {
    div()
        .absolute()
        .left(px(SIDE_PADDING))
        .top(px(top))
        .w(px(BUTTON_WIDTH))
        .text_center()
        .text_size(px(12.))
        .line_height(px(15.))
        .text_color(rgb(if is_error { ERROR_TEXT } else { MUTED_TEXT }))
        .child(label)
}

fn version_label(top: f32) -> Div {
    div()
        .absolute()
        .left(px(SIDE_PADDING))
        .top(px(top))
        .w(px(BUTTON_WIDTH))
        .text_center()
        .text_size(px(11.))
        .line_height(px(14.))
        .text_color(rgb(MUTED_TEXT))
        .child(format!("v{}", platform::CURRENT_VERSION))
}

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

pub fn show_install_window(exe_path: PathBuf) {
    let state = InstallRenderState::probe();
    let window_height = if state.install_state.is_current_version() {
        INSTALLED_WINDOW_HEIGHT
    } else {
        INSTALL_WINDOW_HEIGHT
    };
    let state = Arc::new(Mutex::new(state));
    let notify = Arc::new(Notify::new());

    Application::new().run(move |cx| {
        register_fonts(cx);
        let bounds = Bounds::centered(
            None,
            size(px(INSTALL_WINDOW_WIDTH), px(window_height)),
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
            window_background: WindowBackgroundAppearance::Transparent,
            window_decorations: Some(WindowDecorations::Client),
            ..Default::default()
        };

        cx.open_window(options, move |_, cx| {
            let exe_path = exe_path.clone();
            let state = state.clone();
            let notify = notify.clone();
            cx.new(move |_| {
                InstallWindow::new(
                    exe_path.clone(),
                    state.clone(),
                    notify.clone(),
                )
            })
        })
        .unwrap();

        cx.activate(true);
    });
}
