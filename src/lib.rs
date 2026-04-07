use futures::{StreamExt, stream};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;
use tokio::fs;

/// Cooperative control for copy operations.
///
/// This lets the app pause new work or cancel the remaining queue without
/// rewriting the underlying file-copy algorithm.
#[derive(Clone, Default)]
pub struct CopyController {
    paused: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
}

impl CopyController {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pause(&self) {
        if !self.is_cancelled() {
            self.paused.store(true, Ordering::SeqCst);
        }
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    async fn wait_until_resumed(&self) -> bool {
        while self.is_paused() {
            if self.is_cancelled() {
                return false;
            }

            tokio::time::sleep(Duration::from_millis(80)).await;
        }

        !self.is_cancelled()
    }
}

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

/// Collect files recursively from a file or directory source.
pub async fn collect_files(
    src_root: &Path,
    dst_root: &Path,
) -> anyhow::Result<Vec<(PathBuf, PathBuf)>> {
    // Check whether the source is a file or a directory.
    let metadata = fs::metadata(src_root).await?;

    // Copy a single file directly.
    if metadata.is_file() {
        let file_name = src_root
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Could not read the file name"))?;
        let dst_path = dst_root.join(file_name);
        return Ok(vec![(src_root.to_path_buf(), dst_path)]);
    }

    // Walk directory contents recursively.
    let mut result = Vec::new();
    let mut stack: Vec<(PathBuf, PathBuf)> = Vec::new();

    // Preserve the source root name when available.
    // For drive roots such as `D:\`, `file_name()` returns `None`, so we use
    // the target root directly.
    let initial_dst = match src_root.file_name() {
        Some(src_name) => dst_root.join(src_name),
        None => dst_root.to_path_buf(),
    };

    stack.push((src_root.to_path_buf(), initial_dst));

    while let Some((current_src, current_dst)) = stack.pop() {
        let mut dir = fs::read_dir(&current_src).await?;

        while let Some(entry) = dir.next_entry().await? {
            let entry_path = entry.path();
            let file_type = entry.file_type().await?;

            if file_type.is_dir() {
                let name = entry_path
                    .file_name()
                    .ok_or_else(|| anyhow::anyhow!("Could not read the subdirectory name"))?;
                let dst_sub = current_dst.join(name);
                stack.push((entry_path, dst_sub));
            } else if file_type.is_file() {
                let name = entry_path
                    .file_name()
                    .ok_or_else(|| anyhow::anyhow!("Could not read the file name"))?;
                let dst_file_path = current_dst.join(name);
                result.push((entry_path, dst_file_path));
            }
        }
    }

    Ok(result)
}

/// Create destination directories ahead of time.
pub async fn precreate_directories(files: &[(PathBuf, PathBuf)]) -> anyhow::Result<()> {
    let unique_dirs: HashSet<PathBuf> = files
        .iter()
        .filter_map(|(_, dst)| dst.parent().map(|p| p.to_path_buf()))
        .collect();

    let tasks: Vec<_> = unique_dirs
        .into_iter()
        .map(|dir| async move { fs::create_dir_all(&dir).await })
        .collect();

    futures::future::join_all(tasks)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

    Ok(())
}

/// Resolve the optimal concurrency.
pub fn calculate_concurrency(user_override: Option<usize>) -> usize {
    if let Some(n) = user_override {
        return n.max(1);
    }
    let cores = num_cpus::get();
    (cores * 4).clamp(4, 128)
}

/// Strip the Windows UNC path prefix (`\\?\C:\... -> C:\...`).
pub fn normalize_path(path: PathBuf) -> PathBuf {
    let path_str = path.to_string_lossy();
    if let Some(stripped) = path_str.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path
    }
}

/// Copy files while emitting progress callbacks.
pub async fn copy_files_with_progress(
    files: Vec<(PathBuf, PathBuf)>,
    concurrency: usize,
    callback: Option<ProgressCallback>,
    control: Option<CopyController>,
) -> anyhow::Result<()> {
    let files_processed = Arc::new(AtomicUsize::new(0));
    let control = control.unwrap_or_default();

    stream::iter(files.into_iter().map(|(src, dst)| {
        let callback = callback.as_ref().map(|cb| cb as &ProgressCallback);
        let files_processed = files_processed.clone();
        let control = control.clone();

        async move {
            if control.is_cancelled() {
                return;
            }

            if !control.wait_until_resumed().await {
                return;
            }

            if control.is_cancelled() {
                return;
            }

            // Capture the file name early so failures can still update the UI.
            let file_name = src
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            if let Some(cb) = callback {
                cb(ProgressUpdate {
                    phase: ProgressPhase::Started,
                    processed_files: files_processed.load(Ordering::SeqCst),
                    file_name: file_name.clone(),
                    file_bytes: 0,
                });
            }

            // Resolve the file size before copying.
            let file_size = match fs::metadata(&src).await {
                Ok(meta) => meta.len(),
                Err(e) => {
                    eprintln!("ERROR: failed to read metadata for {:?} | {}", src, e);

                    let processed = files_processed.fetch_add(1, Ordering::SeqCst) + 1;
                    if let Some(cb) = callback {
                        cb(ProgressUpdate {
                            phase: ProgressPhase::Failed,
                            processed_files: processed,
                            file_name,
                            file_bytes: 0,
                        });
                    }

                    return;
                }
            };

            // Copy the file.
            match fs::copy(&src, &dst).await {
                Ok(_) => {
                    let processed = files_processed.fetch_add(1, Ordering::SeqCst) + 1;

                    if let Some(cb) = callback {
                        cb(ProgressUpdate {
                            phase: ProgressPhase::Finished,
                            processed_files: processed,
                            file_name,
                            file_bytes: file_size,
                        });
                    }
                }
                Err(e) => {
                    eprintln!("ERROR: {:?} -> {:?} | {}", src, dst, e);

                    let processed = files_processed.fetch_add(1, Ordering::SeqCst) + 1;
                    if let Some(cb) = callback {
                        cb(ProgressUpdate {
                            phase: ProgressPhase::Failed,
                            processed_files: processed,
                            file_name,
                            file_bytes: file_size,
                        });
                    }
                }
            }
        }
    }))
    .buffer_unordered(concurrency)
    .for_each(|_| async {})
    .await;

    Ok(())
}
