//! Where the running executable lives, and whether that is a place worth
//! writing into the shell integration.
//!
//! Every context-menu entry mcopy registers embeds an absolute path to the
//! executable. Version 0.2 took that path from `current_exe()` unconditionally,
//! so installing from `~/Downloads`, a mounted `.dmg`, or a `cargo` target
//! directory produced menu entries that broke the moment the user cleaned up the
//! download or ejected the image — with no diagnostic, because the entry still
//! existed and simply pointed at nothing.
//!
//! Refusing to register from a volatile path turns that silent breakage into an
//! actionable message at the one moment the user can fix it.

use std::path::{Path, PathBuf};

/// Why a path is not a durable home for the executable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolatileReason {
    /// A read-only or unmountable image: a macOS `.dmg` or an AppImage mount.
    MountedImage,
    /// A system or user temporary directory, subject to cleanup at any time.
    TemporaryDirectory,
    /// The browser download directory.
    DownloadDirectory,
    /// A `cargo` build directory (developer running from `target/`).
    BuildDirectory,
}

impl VolatileReason {
    /// A message telling the user exactly what to do, phrased for the platform.
    pub fn remedy(self) -> &'static str {
        match self {
            Self::MountedImage => {
                if cfg!(target_os = "macos") {
                    "Drag mcopy to your Applications folder, then run it from there."
                } else {
                    "Install mcopy with the provided package, then run the installed copy."
                }
            },
            Self::TemporaryDirectory | Self::DownloadDirectory => {
                if cfg!(target_os = "windows") {
                    "Run the mcopy installer first, then launch mcopy from the Start menu."
                } else if cfg!(target_os = "macos") {
                    "Drag mcopy to your Applications folder, then run it from there."
                } else {
                    "Install the mcopy package first, then launch it from your applications menu."
                }
            },
            Self::BuildDirectory => {
                "Running from a build directory. Install mcopy before registering the menu entries."
            },
        }
    }
}

/// Whether the executable sits somewhere the shell integration can rely on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallLocation {
    /// A canonical, durable install path.
    Installed(PathBuf),
    /// A path that may disappear; registering menu entries here would break.
    Volatile {
        reason: VolatileReason,
        exe: PathBuf,
    },
    /// Somewhere unrecognised. Treated as usable — a user may legitimately keep
    /// the binary in `~/bin` or `/opt` — but recorded so callers can say so.
    Unrecognized(PathBuf),
}

impl InstallLocation {
    /// The executable path, whatever the classification.
    pub fn exe(&self) -> &Path {
        match self {
            Self::Installed(path)
            | Self::Volatile { exe: path, .. }
            | Self::Unrecognized(path) => path,
        }
    }

    /// Whether shell integration may be registered against this path.
    pub fn is_usable(&self) -> bool {
        !matches!(self, Self::Volatile { .. })
    }

    /// The user-facing reason registration is blocked, if it is.
    pub fn blocking_reason(&self) -> Option<VolatileReason> {
        match self {
            Self::Volatile { reason, .. } => Some(*reason),
            _ => None,
        }
    }
}

/// The durable path that represents this process.
///
/// Not the same as `current_exe()` for an AppImage: the runtime mounts the
/// image under `/tmp/.mount_XXXXXX` and runs the binary from inside it, so
/// `current_exe()` reports a path that stops existing the moment the process
/// exits — and that [`classify`] correctly rejects as a mounted image. The
/// runtime always exports `$APPIMAGE` as the absolute path of the `.AppImage`
/// file the user actually keeps, which is what menu entries must point at.
pub fn resolve_exe_path() -> anyhow::Result<PathBuf> {
    #[cfg(target_os = "linux")]
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        let appimage = PathBuf::from(appimage);
        if appimage.is_absolute() {
            return Ok(appimage);
        }
    }

    Ok(std::env::current_exe()?)
}

/// Classify the currently running executable.
pub fn detect() -> anyhow::Result<InstallLocation> {
    Ok(classify(&resolve_exe_path()?))
}

/// Classify an arbitrary path.
///
/// Split from [`detect`] so the rules are testable without moving the test
/// binary around the filesystem.
pub fn classify(exe: &Path) -> InstallLocation {
    if let Some(reason) = volatile_reason(exe) {
        return InstallLocation::Volatile {
            reason,
            exe: exe.to_path_buf(),
        };
    }

    if installed_roots().iter().any(|root| exe.starts_with(root)) {
        return InstallLocation::Installed(exe.to_path_buf());
    }

    InstallLocation::Unrecognized(exe.to_path_buf())
}

/// Canonical install roots for the current platform, most specific first.
fn installed_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    #[cfg(target_os = "windows")]
    {
        if let Some(local) = dirs::data_local_dir() {
            roots.push(local.join("Programs").join("mcopy"));
        }
        for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(base) = std::env::var_os(variable) {
                roots.push(PathBuf::from(base).join("mcopy"));
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        roots.push(PathBuf::from("/Applications/mcopy.app"));
        if let Some(home) = dirs::home_dir() {
            roots.push(home.join("Applications/mcopy.app"));
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        roots.push(PathBuf::from("/usr/bin"));
        roots.push(PathBuf::from("/usr/local/bin"));
        roots.push(PathBuf::from("/opt/mcopy"));
        if let Some(home) = dirs::home_dir() {
            roots.push(home.join(".local/bin"));
        }
    }

    roots
}

/// Detect a path that is expected to disappear.
fn volatile_reason(exe: &Path) -> Option<VolatileReason> {
    // A cargo build directory. Checked first so developers get the accurate
    // message rather than a generic temp-dir one.
    if exe.components().any(|component| {
        matches!(component.as_os_str().to_str(), Some("target"))
    }) && exe.components().any(|component| {
        matches!(component.as_os_str().to_str(), Some("debug" | "release"))
    }) {
        return Some(VolatileReason::BuildDirectory);
    }

    if starts_with_any(exe, &mounted_image_roots()) {
        return Some(VolatileReason::MountedImage);
    }

    if starts_with_any(exe, &temporary_roots()) {
        return Some(VolatileReason::TemporaryDirectory);
    }

    if let Some(downloads) = dirs::download_dir()
        && exe.starts_with(&downloads)
    {
        return Some(VolatileReason::DownloadDirectory);
    }

    None
}

fn mounted_image_roots() -> Vec<PathBuf> {
    // Windows has no equivalent mount convention, so the list is empty there.
    #[allow(unused_mut)]
    let mut roots = Vec::new();

    #[cfg(target_os = "macos")]
    {
        // Disk images mount under /Volumes; the boot volume is not there.
        roots.push(PathBuf::from("/Volumes"));
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        // AppImages extract into a per-run mount point under /tmp.
        roots.push(PathBuf::from("/tmp/.mount_"));
        roots.push(PathBuf::from("/media"));
        roots.push(PathBuf::from("/mnt"));
    }

    roots
}

fn temporary_roots() -> Vec<PathBuf> {
    #[allow(unused_mut)]
    let mut roots = vec![std::env::temp_dir()];

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        roots.push(PathBuf::from("/tmp"));
        roots.push(PathBuf::from("/var/tmp"));
        roots.push(PathBuf::from("/run/user"));
    }

    #[cfg(target_os = "windows")]
    {
        // Archive tools and installers stage payloads here before extraction.
        if let Some(local) = dirs::data_local_dir() {
            roots.push(local.join("Temp"));
        }
    }

    roots
}

fn starts_with_any(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| {
        // `starts_with` compares whole components, which is what we want for
        // real directories. AppImage mount points are a *prefix* of a component
        // name (`/tmp/.mount_mcopyAbC123`), so those are matched textually.
        path.starts_with(root)
            || root.to_str().is_some_and(|root| {
                root.ends_with('_')
                    && path.to_str().is_some_and(|path| path.starts_with(root))
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cargo_build_directory_is_volatile() {
        let exe = if cfg!(windows) {
            PathBuf::from(r"C:\src\mcopy\target\release\mcopy.exe")
        } else {
            PathBuf::from("/home/u/src/mcopy/target/release/mcopy")
        };

        assert_eq!(
            classify(&exe).blocking_reason(),
            Some(VolatileReason::BuildDirectory)
        );
        assert!(!classify(&exe).is_usable());
    }

    #[test]
    fn a_debug_build_directory_is_volatile() {
        let exe = if cfg!(windows) {
            PathBuf::from(r"C:\src\mcopy\target\debug\mcopy.exe")
        } else {
            PathBuf::from("/home/u/src/mcopy/target/debug/mcopy")
        };
        assert_eq!(
            classify(&exe).blocking_reason(),
            Some(VolatileReason::BuildDirectory)
        );
    }

    /// A directory merely *named* `target` is not a build directory; the
    /// `debug`/`release` component is what makes it one.
    #[test]
    fn a_directory_named_target_alone_is_not_a_build_directory() {
        let exe = if cfg!(windows) {
            PathBuf::from(r"C:\work\target\mcopy.exe")
        } else {
            PathBuf::from("/srv/target/mcopy")
        };
        assert_ne!(
            classify(&exe).blocking_reason(),
            Some(VolatileReason::BuildDirectory)
        );
    }

    #[test]
    fn the_system_temp_directory_is_volatile() {
        let exe = std::env::temp_dir().join("mcopy-extract").join("mcopy");
        assert_eq!(
            classify(&exe).blocking_reason(),
            Some(VolatileReason::TemporaryDirectory)
        );
    }

    #[test]
    fn the_download_directory_is_volatile() {
        let Some(downloads) = dirs::download_dir() else {
            return; // No download dir on this machine; nothing to assert.
        };
        let exe = downloads.join("mcopy");
        assert_eq!(
            classify(&exe).blocking_reason(),
            Some(VolatileReason::DownloadDirectory)
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_mounted_disk_image_is_volatile() {
        let exe =
            PathBuf::from("/Volumes/mcopy/mcopy.app/Contents/MacOS/mcopy");
        assert_eq!(
            classify(&exe).blocking_reason(),
            Some(VolatileReason::MountedImage)
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_applications_bundle_is_installed() {
        let exe = PathBuf::from("/Applications/mcopy.app/Contents/MacOS/mcopy");
        assert!(matches!(classify(&exe), InstallLocation::Installed(_)));
        assert!(classify(&exe).is_usable());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn an_appimage_mount_point_is_volatile() {
        let exe = PathBuf::from("/tmp/.mount_mcopyAbC123/usr/bin/mcopy");
        assert_eq!(
            classify(&exe).blocking_reason(),
            Some(VolatileReason::MountedImage)
        );
    }

    /// The whole point of [`resolve_exe_path`]: the `.AppImage` file the user
    /// keeps is durable even though the mount point it runs from is not. If
    /// this regressed, registering from an AppImage would refuse itself.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_appimage_file_itself_is_not_volatile() {
        let exe =
            PathBuf::from("/home/user/Applications/mcopy-x86_64.AppImage");
        let location = classify(&exe);
        assert!(
            location.is_usable(),
            "the AppImage's own path must be registerable"
        );
        assert_eq!(location.blocking_reason(), None);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn system_bin_directories_are_installed() {
        for path in ["/usr/bin/mcopy", "/usr/local/bin/mcopy"] {
            let location = classify(Path::new(path));
            assert!(
                matches!(location, InstallLocation::Installed(_)),
                "{path} should classify as installed"
            );
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn the_per_user_programs_directory_is_installed() {
        let Some(local) = dirs::data_local_dir() else {
            return;
        };
        let exe = local.join("Programs").join("mcopy").join("mcopy.exe");
        assert!(matches!(classify(&exe), InstallLocation::Installed(_)));
    }

    #[test]
    fn an_unrecognized_path_is_usable_but_flagged() {
        let exe = if cfg!(windows) {
            PathBuf::from(r"C:\tools\mcopy.exe")
        } else {
            PathBuf::from("/opt/custom/mcopy")
        };

        let location = classify(&exe);
        assert!(
            location.is_usable(),
            "an unusual but stable path must not block installation"
        );
        assert_eq!(location.blocking_reason(), None);
    }

    #[test]
    fn exe_is_preserved_through_classification() {
        let exe = std::env::temp_dir().join("mcopy");
        assert_eq!(classify(&exe).exe(), exe.as_path());
    }

    #[test]
    fn every_reason_offers_a_remedy() {
        for reason in [
            VolatileReason::MountedImage,
            VolatileReason::TemporaryDirectory,
            VolatileReason::DownloadDirectory,
            VolatileReason::BuildDirectory,
        ] {
            assert!(!reason.remedy().is_empty());
        }
    }
}
