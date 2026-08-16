//! Handing a downloaded artifact to the OS, behind a single `#[cfg]` seam.
//!
//! Mirrors [`crate::platform`]: one trait, one implementation per OS, exactly
//! one place that decides which is compiled in.

use std::path::{Path, PathBuf};

/// What to do with the artifact once it is on disk and verified.
pub trait UpdateInstaller {
    /// Where the download should be written.
    ///
    /// Not always a temporary directory: the Linux implementation needs the
    /// file to land on the same filesystem as the AppImage it replaces so the
    /// final rename is atomic.
    fn download_path(asset_name: &str) -> anyhow::Result<PathBuf>;

    /// Start the install and return once it has been handed off.
    ///
    /// Does not wait for the install to finish — on Windows and macOS an
    /// external installer takes over from here.
    fn install(downloaded: &Path) -> anyhow::Result<()>;

    /// What to tell the user after [`install`](UpdateInstaller::install).
    fn completion_message() -> &'static str;

    /// Whether mcopy should exit so the installer can replace its files.
    fn should_exit_after_install() -> bool;
}

/// A scratch directory for downloads that do not need a specific location.
#[cfg(not(target_os = "linux"))]
fn scratch_download_path(asset_name: &str) -> anyhow::Result<PathBuf> {
    let dir = dirs::cache_dir()
        .ok_or_else(|| anyhow::anyhow!("no cache directory on this system"))?
        .join("mcopy")
        .join("updates");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(asset_name))
}

#[cfg(target_os = "windows")]
mod imp {
    use super::*;

    pub struct WindowsUpdater;

    impl UpdateInstaller for WindowsUpdater {
        fn download_path(asset_name: &str) -> anyhow::Result<PathBuf> {
            scratch_download_path(asset_name)
        }

        /// Run the Inno Setup installer and let it show its own wizard.
        ///
        /// mcopy exits straight after: the installer declares
        /// `CloseApplications=yes`, and quitting first means the user never
        /// sees it ask to close a running mcopy.
        fn install(downloaded: &Path) -> anyhow::Result<()> {
            std::process::Command::new(downloaded).spawn()?;
            Ok(())
        }

        fn completion_message() -> &'static str {
            "The installer is starting. mcopy will close so it can finish."
        }

        fn should_exit_after_install() -> bool {
            true
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;

    pub struct MacosUpdater;

    impl UpdateInstaller for MacosUpdater {
        fn download_path(asset_name: &str) -> anyhow::Result<PathBuf> {
            scratch_download_path(asset_name)
        }

        /// Hand the package to Installer.app.
        ///
        /// Its postinstall script re-registers the Finder Services, so the
        /// integration is refreshed as part of the update.
        fn install(downloaded: &Path) -> anyhow::Result<()> {
            std::process::Command::new("/usr/bin/open")
                .arg(downloaded)
                .spawn()?;
            Ok(())
        }

        fn completion_message() -> &'static str {
            "The installer is open. Follow it to finish updating mcopy."
        }

        fn should_exit_after_install() -> bool {
            true
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    pub struct LinuxUpdater;

    /// The `.AppImage` file the user keeps, as exported by the runtime.
    fn running_appimage() -> anyhow::Result<PathBuf> {
        std::env::var_os("APPIMAGE").map(PathBuf::from).ok_or_else(|| {
            anyhow::anyhow!(
                "Automatic updates need the AppImage build. Download the new \
                 version from the releases page instead."
            )
        })
    }

    impl UpdateInstaller for LinuxUpdater {
        /// Download beside the AppImage being replaced.
        ///
        /// Same directory means same filesystem, which makes the rename in
        /// [`install`](UpdateInstaller::install) atomic — the user never sees a
        /// half-written AppImage under the name they launch.
        fn download_path(asset_name: &str) -> anyhow::Result<PathBuf> {
            let current = running_appimage()?;
            let dir = current.parent().ok_or_else(|| {
                anyhow::anyhow!("could not determine the AppImage's directory")
            })?;
            Ok(dir.join(format!(".{asset_name}.download")))
        }

        /// Replace the running AppImage in place.
        ///
        /// Unix lets a file be replaced while a process is executing it: the
        /// running mcopy keeps its old inode until it exits, and the path now
        /// resolves to the new build. No restart is attempted — the next
        /// launch picks it up.
        fn install(downloaded: &Path) -> anyhow::Result<()> {
            let current = running_appimage()?;

            let mut permissions = std::fs::metadata(downloaded)?.permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(downloaded, permissions)?;

            if std::fs::rename(downloaded, &current).is_err() {
                // Different filesystem: fall back to a copy.
                std::fs::copy(downloaded, &current)?;
                let _ = std::fs::remove_file(downloaded);
            }

            Ok(())
        }

        fn completion_message() -> &'static str {
            "Updated. The new version starts the next time you run mcopy."
        }

        /// The replacement already happened; the running process is unaffected.
        fn should_exit_after_install() -> bool {
            false
        }
    }
}

#[cfg(target_os = "windows")]
pub use imp::WindowsUpdater as Updater;

#[cfg(target_os = "macos")]
pub use imp::MacosUpdater as Updater;

#[cfg(target_os = "linux")]
pub use imp::LinuxUpdater as Updater;

/// Open the releases page for an install we cannot update ourselves.
pub fn open_releases_page() -> anyhow::Result<()> {
    const RELEASES_URL: &str =
        "https://github.com/NAKAMOZ/mcopy/releases/latest";

    #[cfg(target_os = "windows")]
    {
        // `start` is a cmd builtin, not an executable.
        std::process::Command::new("cmd")
            .args(["/C", "start", "", RELEASES_URL])
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/open")
            .arg(RELEASES_URL)
            .spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(RELEASES_URL)
            .spawn()?;
    }

    Ok(())
}
