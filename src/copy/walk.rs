use crate::calculate_concurrency;
use futures::{StreamExt, stream};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Clone, Debug)]
pub struct CopyItem {
    pub src: PathBuf,
    pub dst: PathBuf,
    pub kind: CopyItemKind,
}

#[derive(Clone, Debug)]
pub enum CopyItemKind {
    File,
    Directory,
    Symlink {
        target: PathBuf,
        target_is_dir: bool,
    },
}

impl CopyItem {
    fn file(src: PathBuf, dst: PathBuf) -> Self {
        Self {
            src,
            dst,
            kind: CopyItemKind::File,
        }
    }

    fn directory(src: PathBuf, dst: PathBuf) -> Self {
        Self {
            src,
            dst,
            kind: CopyItemKind::Directory,
        }
    }

    fn symlink(
        src: PathBuf,
        dst: PathBuf,
        target: PathBuf,
        target_is_dir: bool,
    ) -> Self {
        Self {
            src,
            dst,
            kind: CopyItemKind::Symlink {
                target,
                target_is_dir,
            },
        }
    }
}

/// Collect filesystem items recursively from a file or directory source.
pub async fn collect_files(
    src_root: &Path,
    dst_root: &Path,
) -> anyhow::Result<Vec<CopyItem>> {
    // Check whether the source is a file or a directory.
    let metadata = fs::symlink_metadata(src_root).await?;

    let dst_path = match src_root.file_name() {
        Some(src_name) => dst_root.join(src_name),
        None => dst_root.to_path_buf(),
    };

    if metadata.file_type().is_symlink() {
        let target = fs::read_link(src_root).await?;
        let target_is_dir = fs::metadata(src_root)
            .await
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);

        return Ok(vec![CopyItem::symlink(
            src_root.to_path_buf(),
            dst_path,
            target,
            target_is_dir,
        )]);
    }

    // Copy a single file directly.
    if metadata.is_file() {
        return Ok(vec![CopyItem::file(src_root.to_path_buf(), dst_path)]);
    }

    // Walk directory contents recursively, reading each tree level with bounded
    // concurrency so deep/wide trees overlap their (latency-bound) reads instead
    // of stat-ing one directory at a time.
    let mut result = vec![CopyItem::directory(
        src_root.to_path_buf(),
        dst_path.clone(),
    )];

    // Preserve the source root name when available.
    // For drive roots such as `D:\`, `file_name()` returns `None`, so we use
    // the target root directly.
    let initial_dst = dst_path;

    // Bound the fan-out to keep open file descriptors in check on huge trees.
    let concurrency = calculate_concurrency(None);
    let mut current_level: Vec<(PathBuf, PathBuf)> =
        vec![(src_root.to_path_buf(), initial_dst)];

    while !current_level.is_empty() {
        // Read every directory at this level concurrently.
        let level_results: Vec<anyhow::Result<DirContents>> =
            stream::iter(current_level)
                .map(|(src, dst)| read_dir_contents(src, dst))
                .buffer_unordered(concurrency)
                .collect()
                .await;

        let mut next_level = Vec::new();
        for contents in level_results {
            let contents = contents?;
            result.extend(contents.items);
            next_level.extend(contents.subdirs);
        }

        current_level = next_level;
    }

    Ok(result)
}

/// Files and subdirectories discovered in a single directory.
struct DirContents {
    items: Vec<CopyItem>,
    subdirs: Vec<(PathBuf, PathBuf)>,
}

/// Read one directory, mapping each entry's source path to its destination.
async fn read_dir_contents(
    src: PathBuf,
    dst: PathBuf,
) -> anyhow::Result<DirContents> {
    let mut items = Vec::new();
    let mut subdirs = Vec::new();
    let mut dir = fs::read_dir(&src).await?;

    while let Some(entry) = dir.next_entry().await? {
        let entry_path = entry.path();
        let file_type = entry.file_type().await?;
        let dst_entry =
            dst.join(entry_path.file_name().ok_or_else(|| {
                anyhow::anyhow!("Could not read the entry name")
            })?);

        if file_type.is_dir() {
            items.push(CopyItem::directory(
                entry_path.clone(),
                dst_entry.clone(),
            ));
            subdirs.push((entry_path, dst_entry));
        } else if file_type.is_file() {
            items.push(CopyItem::file(entry_path, dst_entry));
        } else if file_type.is_symlink() {
            let target = fs::read_link(&entry_path).await?;
            let target_is_dir = fs::metadata(&entry_path)
                .await
                .map(|metadata| metadata.is_dir())
                .unwrap_or(false);

            items.push(CopyItem::symlink(
                entry_path,
                dst_entry,
                target,
                target_is_dir,
            ));
        }
    }

    Ok(DirContents { items, subdirs })
}

/// Create destination directories ahead of time.
pub async fn precreate_directories(items: &[CopyItem]) -> anyhow::Result<()> {
    let unique_dirs: HashSet<PathBuf> = items
        .iter()
        .filter_map(|item| item.dst.parent().map(|p| p.to_path_buf()))
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
