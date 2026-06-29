//! OS integration (context-menu install/uninstall) behind a single trait seam.
//!
//! The rest of the app never sees a `#[cfg]`: it talks to [`Platform`], which
//! is the active OS implementation selected in exactly one place below.

pub mod state;

pub use state::{CURRENT_VERSION, ContextMenuInstallState};

use std::path::Path;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

/// Context-menu integration for one operating system.
///
/// Unsupported platforms inherit the default methods, which report
/// "not supported" / `NotInstalled`.
pub trait ContextMenu {
    fn install(exe_path: &Path) -> anyhow::Result<()> {
        let _ = exe_path;
        anyhow::bail!("Context menu integration is not supported on this platform")
    }

    fn uninstall() -> anyhow::Result<()> {
        anyhow::bail!("Context menu integration is not supported on this platform")
    }

    fn state() -> anyhow::Result<ContextMenuInstallState> {
        Ok(ContextMenuInstallState::NotInstalled)
    }
}

/// Fallback used on platforms without a dedicated implementation.
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub struct Unsupported;
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
impl ContextMenu for Unsupported {}

// The single `#[cfg]` selection point for the active OS implementation.
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub use Unsupported as Platform;
#[cfg(target_os = "linux")]
pub use linux::LinuxMenu as Platform;
#[cfg(target_os = "macos")]
pub use macos::MacosMenu as Platform;
#[cfg(target_os = "windows")]
pub use windows::WindowsMenu as Platform;

/// Install the context menu, replacing a stale version if one is present.
pub fn install_or_update_context_menu(exe_path: &Path) -> anyhow::Result<()> {
    if Platform::state()?.is_current_version() {
        return Ok(());
    }

    Platform::uninstall()?;
    Platform::install(exe_path)
}
