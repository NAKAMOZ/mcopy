//! mcopy core library.
//!
//! One responsibility per module, platform code behind a single trait seam:
//! - [`copy`] — the headless file-copy engine (controller, progress, walk).
//! - [`clipboard`] — the owned copy/paste state model and its clipboard mirror.
//! - [`platform`] — OS integration behind the `ContextMenu` trait, plus
//!   install-location classification.
//! - [`ui`] — GPUI windows, with state separated from view.
//! - [`util`] — leaf helpers (path normalization, shell escaping, logging).
//!
//! The re-exports below keep `mcopy::CopyController` and friends resolvable.

pub mod clipboard;
pub mod copy;
pub mod platform;
pub mod ui;
pub mod util;

/// Reverse-DNS identifier for the application.
///
/// One constant because every platform keys something different off it and they
/// must agree: the macOS bundle identifier and installer package id, the
/// Wayland/X11 `app_id` that pairs a window with its `.desktop` entry, that
/// `.desktop` file's own name, and the AppStream component id used by GNOME
/// Software, KDE Discover and Flathub. A mismatch shows up as a window with a
/// missing icon or an app the software centre cannot attribute to anyone.
///
/// The `io.github.` form is the correct namespace for a project hosted on
/// GitHub without its own registered domain; Flathub requires exactly this
/// shape, and it avoids asserting ownership of a domain we do not control.
pub const APP_ID: &str = "io.github.nakamoz.mcopy";

/// Human-readable publisher, shown in installers and package metadata.
pub const APP_PUBLISHER: &str = "NAKAMOZ";

/// Copyright line for installers, bundles and the Windows version resource.
pub const APP_COPYRIGHT: &str = "Copyright (c) 2026 Nevzat ÇELİKKANAT";

pub use copy::{
    CopyController, CopyErrorKind, CopyItem, CopyItemKind, ProgressCallback,
    ProgressPhase, ProgressUpdate, collect_files, copy_files_with_progress,
    precreate_directories,
};
pub use util::path::{
    calculate_concurrency, normalize_path, repair_shell_argument,
};

#[cfg(test)]
mod identity_tests {
    use super::*;

    /// The `io.github.<owner>.<project>` shape is what Flathub requires and
    /// what lets every platform agree on one identity.
    #[test]
    fn app_id_is_a_reverse_dns_identifier() {
        assert_eq!(APP_ID, "io.github.nakamoz.mcopy");
        assert_eq!(APP_ID.split('.').count(), 4);
        assert!(APP_ID.chars().all(|c| c.is_ascii_lowercase() || c == '.'));
    }

    #[test]
    fn publisher_and_copyright_are_populated() {
        assert!(!APP_PUBLISHER.is_empty());
        assert!(APP_COPYRIGHT.starts_with("Copyright (c)"));
    }

    /// `build.rs` cannot depend on this crate, so it carries its own copy of the
    /// publisher and copyright strings. It exports what it embedded; this pins
    /// the two together so the shipped binary's file properties can never
    /// disagree with the rest of the packaging.
    #[cfg(windows)]
    #[test]
    fn the_windows_version_resource_matches_these_constants() {
        assert_eq!(env!("MCOPY_RESOURCE_PUBLISHER"), APP_PUBLISHER);
        assert_eq!(env!("MCOPY_RESOURCE_COPYRIGHT"), APP_COPYRIGHT);
    }
}
