mod controller;
mod progress;
mod walk;

pub use controller::CopyController;
pub use progress::{ProgressCallback, ProgressPhase, ProgressUpdate};
pub use walk::{CopyItem, CopyItemKind, collect_files, precreate_directories};

use futures::{StreamExt, stream};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::fs;

/// Copy filesystem items while emitting progress callbacks.
pub async fn copy_files_with_progress(
    items: Vec<CopyItem>,
    concurrency: usize,
    callback: Option<ProgressCallback>,
    control: Option<CopyController>,
) -> anyhow::Result<()> {
    let items_processed = Arc::new(AtomicUsize::new(0));
    let control = control.unwrap_or_default();

    stream::iter(items.into_iter().map(|item| {
        let callback = callback.as_ref().map(|cb| cb as &ProgressCallback);
        let items_processed = items_processed.clone();
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
            let file_name = item
                .src
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            if let Some(cb) = callback {
                cb(ProgressUpdate {
                    phase: ProgressPhase::Started,
                    processed_files: items_processed.load(Ordering::Relaxed),
                    file_name: file_name.clone(),
                    file_bytes: 0,
                });
            }

            match copy_item(&item).await {
                Ok(bytes) => {
                    let processed = items_processed.fetch_add(1, Ordering::Relaxed) + 1;

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
                    eprintln!("ERROR: {:?} -> {:?} | {}", item.src, item.dst, e);

                    let processed = items_processed.fetch_add(1, Ordering::Relaxed) + 1;
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

async fn copy_item(item: &CopyItem) -> anyhow::Result<u64> {
    match &item.kind {
        CopyItemKind::File => {
            // `fs::copy` returns the byte count, so no separate `metadata` stat
            // is needed just to learn the size.
            Ok(fs::copy(&item.src, &item.dst).await?)
        }
        CopyItemKind::Directory => {
            fs::create_dir_all(&item.dst).await?;
            Ok(0)
        }
        CopyItemKind::Symlink {
            target,
            target_is_dir,
        } => {
            remove_existing_file_or_symlink(&item.dst).await?;
            create_symlink(target, &item.dst, *target_is_dir).await?;
            Ok(0)
        }
    }
}

async fn remove_existing_file_or_symlink(path: &std::path::Path) -> anyhow::Result<()> {
    let Ok(metadata) = fs::symlink_metadata(path).await else {
        return Ok(());
    };

    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path).await?;
    }

    Ok(())
}

#[cfg(unix)]
async fn create_symlink(
    target: &std::path::Path,
    link: &std::path::Path,
    _target_is_dir: bool,
) -> anyhow::Result<()> {
    Ok(tokio::fs::symlink(target, link).await?)
}

#[cfg(windows)]
async fn create_symlink(
    target: &std::path::Path,
    link: &std::path::Path,
    target_is_dir: bool,
) -> anyhow::Result<()> {
    if target_is_dir {
        tokio::fs::symlink_dir(target, link).await?;
    } else {
        tokio::fs::symlink_file(target, link).await?;
    }

    Ok(())
}
