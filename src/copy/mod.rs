mod controller;
mod progress;
mod walk;

pub use controller::CopyController;
pub use progress::{ProgressCallback, ProgressPhase, ProgressUpdate};
pub use walk::{collect_files, precreate_directories};

use futures::{StreamExt, stream};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::fs;

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
