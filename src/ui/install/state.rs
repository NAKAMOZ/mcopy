use crate::platform::{self, ContextMenu, ContextMenuInstallState, Platform};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstallOperation {
    Install,
    Uninstall,
}

#[derive(Clone)]
pub(crate) struct InstallRenderState {
    pub install_state: ContextMenuInstallState,
    pub active_operation: Option<InstallOperation>,
    pub message: String,
    pub is_error: bool,
}

impl InstallRenderState {
    /// Build the initial state by probing the current install status.
    pub(crate) fn probe() -> Self {
        Platform::state()
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
            })
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.active_operation.is_some()
    }
}

/// Kick off an install/uninstall on a worker thread. Returns `false` (without
/// starting anything) when the operation is not applicable right now.
pub(crate) fn start_operation(
    state: Arc<Mutex<InstallRenderState>>,
    notify: Arc<Notify>,
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
        let refreshed_state = Platform::state();
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
        drop(state);

        // Wake the UI loop so it repaints the finished result.
        notify.notify_waiters();
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

    platform::install_or_update_context_menu(exe_path)
}

fn perform_uninstall(exe_path: &Path) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        if !is_elevated::is_elevated() {
            return run_elevated_command(exe_path, "uninstall");
        }
    }

    let _ = exe_path;
    Platform::uninstall()
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
