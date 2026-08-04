use crate::copy::CopyErrorKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressPhase {
    Started,
    Finished,
    Failed,
}

/// Progress update.
///
/// `processed_files` counts both successful and failed filesystem items.
#[derive(Clone, Debug)]
pub struct ProgressUpdate {
    pub phase: ProgressPhase,
    pub processed_files: usize,
    pub file_name: String,
    pub file_bytes: u64,
    /// Why the item failed. `Some` exactly when `phase` is
    /// [`ProgressPhase::Failed`], so the UI can name the cause instead of
    /// reporting an anonymous count.
    pub error: Option<CopyErrorKind>,
}

/// Progress callback type.
pub type ProgressCallback = Box<dyn Fn(ProgressUpdate) + Send + Sync>;
