#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressPhase {
    Started,
    Finished,
    Failed,
}

/// Progress update.
///
/// `processed_files` counts both successful and failed files.
#[derive(Clone, Debug)]
pub struct ProgressUpdate {
    pub phase: ProgressPhase,
    pub processed_files: usize,
    pub file_name: String,
    pub file_bytes: u64,
}

/// Progress callback type.
pub type ProgressCallback = Box<dyn Fn(ProgressUpdate) + Send + Sync>;
