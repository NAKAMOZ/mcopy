//! mcopy core library.
//!
//! One responsibility per module, platform code behind a single trait seam:
//! - [`copy`] — the headless file-copy engine (controller, progress, walk).
//! - [`clipboard`] — clipboard payload + session-timeout handling.
//! - [`platform`] — OS context-menu integration behind the `ContextMenu` trait.
//! - [`ui`] — GPUI windows, with state separated from view.
//! - [`util`] — leaf helpers (path normalization, concurrency).
//!
//! The re-exports below keep `mcopy::CopyController` and friends resolvable.

pub mod clipboard;
pub mod copy;
pub mod platform;
pub mod ui;
pub mod util;

pub use copy::{
    CopyController, CopyItem, CopyItemKind, ProgressCallback, ProgressPhase,
    ProgressUpdate, collect_files, copy_files_with_progress,
    precreate_directories,
};
pub use util::path::{calculate_concurrency, normalize_path};
