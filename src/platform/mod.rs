//! OS integration (context-menu install/uninstall) behind a single trait seam.
//!
//! The rest of the app never sees a `#[cfg]`: it talks to [`Platform`], which
//! is the active OS implementation selected in exactly one place below.

pub mod location;
pub mod state;

pub use location::{InstallLocation, VolatileReason};
pub use state::{CURRENT_VERSION, ContextMenuInstallState};

use crate::log_info;
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
        anyhow::bail!(
            "Context menu integration is not supported on this platform"
        )
    }

    fn uninstall() -> anyhow::Result<()> {
        anyhow::bail!(
            "Context menu integration is not supported on this platform"
        )
    }

    fn state() -> anyhow::Result<ContextMenuInstallState> {
        Ok(ContextMenuInstallState::NotInstalled)
    }

    /// Show or hide the "Paste with mcopy" entries.
    ///
    /// Called whenever the copy state changes, so the verb is only offered when
    /// pasting would actually do something. Platforms that cannot toggle a menu
    /// entry at runtime report [`PasteVisibility::Unsupported`] and rely on the
    /// paste command itself explaining that there is nothing to paste.
    fn set_paste_available(available: bool) -> anyhow::Result<PasteVisibility> {
        let _ = available;
        Ok(PasteVisibility::Unsupported)
    }
}

/// Whether a platform could actually apply the requested paste-verb visibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteVisibility {
    /// The entry was shown or hidden as asked.
    Applied,
    /// The platform has no runtime mechanism for this; the entry stays visible.
    Unsupported,
}

/// Fallback used on platforms without a dedicated implementation.
#[cfg(not(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "linux"
)))]
pub struct Unsupported;
#[cfg(not(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "linux"
)))]
impl ContextMenu for Unsupported {}

// The single `#[cfg]` selection point for the active OS implementation.
#[cfg(not(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "linux"
)))]
pub use Unsupported as Platform;
#[cfg(target_os = "linux")]
pub use linux::LinuxMenu as Platform;
#[cfg(target_os = "macos")]
pub use macos::MacosMenu as Platform;
#[cfg(target_os = "windows")]
pub use windows::WindowsMenu as Platform;
#[cfg(target_os = "windows")]
pub use windows::{legacy_hklm_entries_present, uninstall_all_users};

/// Install the context menu, replacing a stale version if one is present.
///
/// Refuses to register entries pointing at a path that is expected to
/// disappear, because the resulting menu items would break silently. See
/// [`location`].
pub fn install_or_update_context_menu(exe_path: &Path) -> anyhow::Result<()> {
    let location = location::classify(exe_path);
    if let Some(reason) = location.blocking_reason() {
        anyhow::bail!(
            "mcopy is running from a location that will not persist. {}",
            reason.remedy()
        );
    }

    if Platform::state()?.is_current_version() {
        log_info!("context menu already at version {CURRENT_VERSION}");
        return Ok(());
    }

    Platform::uninstall()?;
    Platform::install(exe_path)?;
    log_info!(
        "context menu installed at version {CURRENT_VERSION} for {}",
        exe_path.display()
    );
    Ok(())
}

/// Reflect the current copy state in the shell menus.
///
/// Best-effort by design: failing to hide a menu entry must never abort a copy
/// or a paste, so the outcome is logged rather than propagated.
pub fn sync_paste_visibility(available: bool) {
    match Platform::set_paste_available(available) {
        Ok(PasteVisibility::Applied) => {
            log_info!("paste menu entries visible={available}")
        },
        Ok(PasteVisibility::Unsupported) => {},
        Err(error) => {
            crate::log_warn!("could not update paste menu visibility: {error}")
        },
    }
}
