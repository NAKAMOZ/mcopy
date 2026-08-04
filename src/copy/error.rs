//! Classification of per-item copy failures.
//!
//! The copy queue is deliberately fault-tolerant: one unreadable file must not
//! abort a 50,000-item paste. Version 0.2 paid for that by writing the reason to
//! stderr — which no user ever sees, because the Windows build has no console
//! and the macOS/Linux shell integrations discard output — and showing only
//! "N items failed". Every permission problem, full disk and vanished source
//! looked identical.
//!
//! Collapsing `io::Error` into a small set of kinds lets the progress window
//! name the actual cause and, where the platform makes it likely, say what to do
//! about it.

use std::io;

/// Why a single item could not be copied.
///
/// Ordered least to most specific; [`CopyErrorKind::dominant`] uses that order
/// to decide which cause to headline when a run hits several.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CopyErrorKind {
    /// Anything not worth calling out individually.
    Other,
    /// The source disappeared between planning and copying.
    NotFound,
    /// The destination filesystem is mounted read-only.
    ReadOnly,
    /// The volume ran out of space.
    NoSpace,
    /// The OS refused access: file permissions, ACLs, or a sandbox policy.
    PermissionDenied,
}

impl CopyErrorKind {
    /// Map an I/O failure onto a user-facing cause.
    pub fn classify(error: &io::Error) -> Self {
        match error.kind() {
            io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            io::ErrorKind::NotFound => Self::NotFound,
            io::ErrorKind::ReadOnlyFilesystem => Self::ReadOnly,
            io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => {
                Self::NoSpace
            },
            _ => Self::Other,
        }
    }

    /// Pull the classification out of an [`anyhow::Error`] that wraps an
    /// `io::Error`, falling back to [`CopyErrorKind::Other`].
    pub fn from_anyhow(error: &anyhow::Error) -> Self {
        error
            .downcast_ref::<io::Error>()
            .map(Self::classify)
            .unwrap_or(Self::Other)
    }

    /// Short phrase naming the cause, for the progress banner.
    pub fn describe(self) -> &'static str {
        match self {
            Self::PermissionDenied => "permission denied",
            Self::NotFound => "source no longer exists",
            Self::ReadOnly => "destination is read-only",
            Self::NoSpace => "not enough free space",
            Self::Other => "see the mcopy log for details",
        }
    }

    /// A concrete next step, when the platform suggests one.
    ///
    /// On macOS a `PermissionDenied` on an ordinary user folder is far more
    /// often a TCC (privacy) denial than a file-mode problem, and the fix lives
    /// in System Settings rather than in `chmod`, so it is worth naming.
    pub fn hint(self) -> Option<&'static str> {
        match self {
            Self::PermissionDenied if cfg!(target_os = "macos") => Some(
                "Grant mcopy Full Disk Access in System Settings > Privacy & Security.",
            ),
            Self::PermissionDenied if cfg!(target_os = "windows") => Some(
                "Check that the destination is not managed by Controlled Folder Access.",
            ),
            Self::PermissionDenied => Some(
                "Check the ownership and mode of the destination directory.",
            ),
            Self::NoSpace => Some("Free up space on the destination volume."),
            Self::ReadOnly => Some("Remount the destination as writable."),
            Self::NotFound | Self::Other => None,
        }
    }

    /// Whether this cause should be presented as an error rather than a note.
    pub fn is_actionable(self) -> bool {
        !matches!(self, Self::Other)
    }

    /// The most specific cause among those observed.
    ///
    /// A run that hits one permission denial and 400 vanished files is best
    /// summarized by the permission denial, because that is the one the user can
    /// act on.
    pub fn dominant(kinds: impl IntoIterator<Item = Self>) -> Option<Self> {
        kinds.into_iter().max()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn io_error(kind: io::ErrorKind) -> io::Error {
        io::Error::new(kind, "test")
    }

    #[test]
    fn classifies_the_kinds_we_surface() {
        assert_eq!(
            CopyErrorKind::classify(&io_error(io::ErrorKind::PermissionDenied)),
            CopyErrorKind::PermissionDenied
        );
        assert_eq!(
            CopyErrorKind::classify(&io_error(io::ErrorKind::NotFound)),
            CopyErrorKind::NotFound
        );
        assert_eq!(
            CopyErrorKind::classify(&io_error(
                io::ErrorKind::ReadOnlyFilesystem
            )),
            CopyErrorKind::ReadOnly
        );
        assert_eq!(
            CopyErrorKind::classify(&io_error(io::ErrorKind::StorageFull)),
            CopyErrorKind::NoSpace
        );
        assert_eq!(
            CopyErrorKind::classify(&io_error(io::ErrorKind::QuotaExceeded)),
            CopyErrorKind::NoSpace
        );
    }

    #[test]
    fn unknown_kinds_fall_back_to_other() {
        assert_eq!(
            CopyErrorKind::classify(&io_error(io::ErrorKind::BrokenPipe)),
            CopyErrorKind::Other
        );
    }

    #[test]
    fn recovers_the_kind_through_an_anyhow_wrapper() {
        let wrapped: anyhow::Error =
            io_error(io::ErrorKind::PermissionDenied).into();
        assert_eq!(
            CopyErrorKind::from_anyhow(&wrapped),
            CopyErrorKind::PermissionDenied
        );

        let opaque = anyhow::anyhow!("not an io error");
        assert_eq!(CopyErrorKind::from_anyhow(&opaque), CopyErrorKind::Other);
    }

    #[test]
    fn dominant_prefers_the_actionable_cause() {
        let observed =
            [CopyErrorKind::NotFound, CopyErrorKind::PermissionDenied];
        assert_eq!(
            CopyErrorKind::dominant(observed),
            Some(CopyErrorKind::PermissionDenied)
        );

        assert_eq!(
            CopyErrorKind::dominant([
                CopyErrorKind::Other,
                CopyErrorKind::NoSpace
            ]),
            Some(CopyErrorKind::NoSpace)
        );
    }

    #[test]
    fn dominant_of_nothing_is_none() {
        assert_eq!(CopyErrorKind::dominant([]), None);
    }

    #[test]
    fn every_kind_has_a_description() {
        for kind in [
            CopyErrorKind::Other,
            CopyErrorKind::NotFound,
            CopyErrorKind::ReadOnly,
            CopyErrorKind::NoSpace,
            CopyErrorKind::PermissionDenied,
        ] {
            assert!(!kind.describe().is_empty());
        }
    }

    #[test]
    fn permission_denial_always_offers_a_next_step() {
        assert!(CopyErrorKind::PermissionDenied.hint().is_some());
        assert!(CopyErrorKind::NotFound.hint().is_none());
    }
}
