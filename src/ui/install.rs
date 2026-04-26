use crate::context_menu::{self, ContextMenuInstallState};
use gpui::*;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const INSTALL_WINDOW_WIDTH: f32 = 300.0;
const INSTALL_WINDOW_HEIGHT: f32 = 240.0;
const INSTALLED_WINDOW_HEIGHT: f32 = 286.0;
const SIDE_PADDING: f32 = 24.0;
const BUTTON_WIDTH: f32 = INSTALL_WINDOW_WIDTH - (SIDE_PADDING * 2.0);
const BUTTON_HEIGHT: f32 = 39.0;

const CARD_BG: u32 = 0xffffff;
const TITLE_TEXT: u32 = 0x111111;
const MUTED_TEXT: u32 = 0x999999;
const SUCCESS_FILL: u32 = 0x22c55e;
const SUCCESS_HOVER: u32 = 0x20b956;
const BLACK_FILL: u32 = 0x000000;
const BLACK_HOVER: u32 = 0x1a1a1a;
const DISABLED_BG: u32 = 0xe5e5e5;
const ERROR_TEXT: u32 = 0x8a8a8a;

struct InstallAssets;

impl AssetSource for InstallAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(match path {
            "logo.svg" => Some(Cow::Borrowed(include_bytes!("../../logo.svg"))),
            _ => None,
        })
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(if path.is_empty() {
            vec![SharedString::from("logo.svg")]
        } else {
            Vec::new()
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InstallOperation {
    Install,
    Uninstall,
}

#[derive(Clone)]
struct InstallRenderState {
    install_state: ContextMenuInstallState,
    active_operation: Option<InstallOperation>,
    message: String,
    is_error: bool,
}

impl InstallRenderState {
    fn is_busy(&self) -> bool {
        self.active_operation.is_some()
    }
}

pub struct InstallWindow {
    exe_path: PathBuf,
    state: Arc<Mutex<InstallRenderState>>,
    refresh_loop_started: bool,
    close_guard_registered: bool,
}

impl InstallWindow {
    fn new(exe_path: PathBuf, state: Arc<Mutex<InstallRenderState>>) -> Self {
        Self {
            exe_path,
            state,
            refresh_loop_started: false,
            close_guard_registered: false,
        }
    }

    fn ensure_refresh_loop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.refresh_loop_started {
            return;
        }

        self.refresh_loop_started = true;
        window
            .spawn(cx, async move |cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(120))
                        .await;

                    if cx.update(|window, _| window.refresh()).is_err() {
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
        let state = self.state.clone();
        window.on_window_should_close(cx, move |_, _| !state.lock().unwrap().is_busy());
    }
}

impl Render for InstallWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_refresh_loop(window, cx);
        self.ensure_close_guard(window, cx);

        let snapshot = self.state.lock().unwrap().clone();
        let visual = resolve_install_visual(&snapshot);

        window.set_window_title(visual.window_title);
        window.resize(size(px(INSTALL_WINDOW_WIDTH), px(visual.window_height)));

        let state = self.state.clone();
        let exe_path = self.exe_path.clone();
        let install_cta = install_action_button(
            "install-mcopy",
            visual.install_label,
            visual.install_disabled,
            visual.install_background,
            visual.install_hover,
            visual.install_text,
            move |_, window, _| {
                if start_operation(state.clone(), exe_path.clone(), InstallOperation::Install) {
                    window.refresh();
                }
            },
        );

        let state = self.state.clone();
        let exe_path = self.exe_path.clone();
        let uninstall_cta = install_action_button(
            "uninstall-mcopy",
            "Uninstall",
            snapshot.is_busy(),
            BLACK_FILL,
            BLACK_HOVER,
            CARD_BG,
            move |_, window, _| {
                if start_operation(state.clone(), exe_path.clone(), InstallOperation::Uninstall) {
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
                    .h(px(108.))
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
            card = card.child(status_label(status, snapshot.is_error, visual.status_top));
        }

        if visual.show_uninstall {
            card = card.child(
                div()
                    .absolute()
                    .left(px(SIDE_PADDING))
                    .top(px(197.))
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
            status_top: 128.0,
            install_label: "Installing",
            install_disabled: true,
            install_background: DISABLED_BG,
            install_hover: DISABLED_BG,
            install_text: MUTED_TEXT,
            install_button_top: 150.0,
            version_top: 206.0,
            show_uninstall: false,
        },
        Some(InstallOperation::Uninstall) => InstallVisual {
            window_title: "mcopy - Uninstalling",
            window_height: INSTALLED_WINDOW_HEIGHT,
            status_line: Some("Uninstalling".to_string()),
            status_top: 128.0,
            install_label: "Install",
            install_disabled: true,
            install_background: DISABLED_BG,
            install_hover: DISABLED_BG,
            install_text: MUTED_TEXT,
            install_button_top: 150.0,
            version_top: 252.0,
            show_uninstall: true,
        },
        None if state.install_state.is_current_version() => InstallVisual {
            window_title: "mcopy - Already Installed",
            window_height: INSTALLED_WINDOW_HEIGHT,
            status_line: Some("Already installed".to_string()),
            status_top: 128.0,
            install_label: "Install",
            install_disabled: true,
            install_background: DISABLED_BG,
            install_hover: DISABLED_BG,
            install_text: MUTED_TEXT,
            install_button_top: 150.0,
            version_top: 252.0,
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
            status_top: 128.0,
            install_label: "Install",
            install_disabled: false,
            install_background: SUCCESS_FILL,
            install_hover: SUCCESS_HOVER,
            install_text: CARD_BG,
            install_button_top: 150.0,
            version_top: 206.0,
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
                .child(img("logo.svg").w_full().h_full()),
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
            .on_click(|_, window, _| window.remove_window())
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
        .child(format!("v{}", context_menu::CURRENT_VERSION))
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

fn start_operation(
    state: Arc<Mutex<InstallRenderState>>,
    exe_path: PathBuf,
    operation: InstallOperation,
) -> bool {
    {
        let mut state = state.lock().unwrap();
        if state.is_busy() {
            return false;
        }

        if operation == InstallOperation::Install && state.install_state.is_current_version() {
            return false;
        }

        if operation == InstallOperation::Uninstall && !state.install_state.is_current_version() {
            return false;
        }

        state.active_operation = Some(operation);
        state.message.clear();
        state.is_error = false;
    }

    std::thread::spawn(move || {
        let result = match operation {
            InstallOperation::Install => perform_install(&exe_path),
            InstallOperation::Uninstall => perform_uninstall(&exe_path),
        };
        let refreshed_state = context_menu::context_menu_install_state();
        let mut state = state.lock().unwrap();

        state.active_operation = None;
        if let Ok(install_state) = refreshed_state {
            state.install_state = install_state;
        }

        match result {
            Ok(()) => {
                state.message.clear();
                state.is_error = false;
            }
            Err(error) => {
                state.message = format!("{}", error);
                state.is_error = true;
            }
        }
    });

    true
}

fn perform_install(exe_path: &Path) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        if !is_elevated::is_elevated() {
            return run_elevated_command(exe_path, "install");
        }
    }

    context_menu::install_or_update_context_menu(exe_path)
}

fn perform_uninstall(exe_path: &Path) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        if !is_elevated::is_elevated() {
            return run_elevated_command(exe_path, "uninstall");
        }
    }

    context_menu::uninstall_context_menu()
}

#[cfg(target_os = "windows")]
fn run_elevated_command(exe_path: &Path, command: &str) -> anyhow::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, INFINITE, WaitForSingleObject,
    };
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let verb = wide_null("runas");
    let file: Vec<u16> = exe_path.as_os_str().encode_wide().chain([0]).collect();
    let parameters = wide_null(command);

    let mut execute_info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpParameters: parameters.as_ptr(),
        nShow: SW_HIDE,
        ..Default::default()
    };

    let started = unsafe { ShellExecuteExW(&mut execute_info) };
    if started == 0 {
        let error = unsafe { GetLastError() };
        anyhow::bail!("could not start elevated command: Windows error {}", error);
    }

    let mut exit_code = 0;
    unsafe {
        WaitForSingleObject(execute_info.hProcess, INFINITE);
        GetExitCodeProcess(execute_info.hProcess, &mut exit_code);
        CloseHandle(execute_info.hProcess);
    }

    if exit_code == 0 {
        Ok(())
    } else {
        anyhow::bail!("elevated command exited with code {}", exit_code)
    }
}

#[cfg(target_os = "windows")]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

pub fn show_install_window(exe_path: PathBuf) {
    let state = context_menu::context_menu_install_state()
        .map(|install_state| InstallRenderState {
            install_state,
            active_operation: None,
            message: String::new(),
            is_error: false,
        })
        .unwrap_or_else(|error| InstallRenderState {
            install_state: ContextMenuInstallState::NotInstalled,
            active_operation: None,
            message: format!("{}", error),
            is_error: true,
        });
    let window_height = if state.install_state.is_current_version() {
        INSTALLED_WINDOW_HEIGHT
    } else {
        INSTALL_WINDOW_HEIGHT
    };
    let state = Arc::new(Mutex::new(state));

    Application::new()
        .with_assets(InstallAssets)
        .run(move |cx| {
            let bounds =
                Bounds::centered(None, size(px(INSTALL_WINDOW_WIDTH), px(window_height)), cx);
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
                cx.new(move |_| InstallWindow::new(exe_path.clone(), state.clone()))
            })
            .unwrap();

            cx.activate(true);
        });
}
