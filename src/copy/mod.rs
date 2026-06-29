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
                    processed_files: files_processed.load(Ordering::Relaxed),
                    file_name: file_name.clone(),
                    file_bytes: 0,
                });
            }

            // Copy the file. `fs::copy` returns the byte count, so no separate
            // `metadata` stat is needed just to learn the size.
            match fs::copy(&src, &dst).await {
                Ok(bytes) => {
                    let processed = files_processed.fetch_add(1, Ordering::Relaxed) + 1;

                    if let Some(cb) = callback {
                        cb(ProgressUpdate {
                            phase: ProgressPhase::Finished,
                            processed_files: processed,
                            file_name,
                            file_bytes: bytes,
                        });
                    }
                }
                Err(e) => {
                    eprintln!("ERROR: {:?} -> {:?} | {}", src, dst, e);

                    let processed = files_processed.fetch_add(1, Ordering::Relaxed) + 1;
                    if let Some(cb) = callback {
                        cb(ProgressUpdate {
                            phase: ProgressPhase::Failed,
                            processed_files: processed,
                            file_name,
                            file_bytes: 0,
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
