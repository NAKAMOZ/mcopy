use crate::calculate_concurrency;
use futures::{StreamExt, stream};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs;

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

    // Walk directory contents recursively, reading each tree level with bounded
    // concurrency so deep/wide trees overlap their (latency-bound) reads instead
    // of stat-ing one directory at a time.
    let mut result = Vec::new();

    // Preserve the source root name when available.
    // For drive roots such as `D:\`, `file_name()` returns `None`, so we use
    // the target root directly.
    let initial_dst = match src_root.file_name() {
        Some(src_name) => dst_root.join(src_name),
        None => dst_root.to_path_buf(),
    };

    // Bound the fan-out to keep open file descriptors in check on huge trees.
    let concurrency = calculate_concurrency(None);
    let mut current_level: Vec<(PathBuf, PathBuf)> = vec![(src_root.to_path_buf(), initial_dst)];

    while !current_level.is_empty() {
        // Read every directory at this level concurrently.
        let level_results: Vec<anyhow::Result<DirContents>> = stream::iter(current_level)
            .map(|(src, dst)| read_dir_contents(src, dst))
            .buffer_unordered(concurrency)
            .collect()
            .await;

        let mut next_level = Vec::new();
        for contents in level_results {
            let contents = contents?;
            result.extend(contents.files);
            next_level.extend(contents.subdirs);
        }

        current_level = next_level;
    }

    Ok(result)
}

/// Files and subdirectories discovered in a single directory.
struct DirContents {
    files: Vec<(PathBuf, PathBuf)>,
    subdirs: Vec<(PathBuf, PathBuf)>,
}

/// Read one directory, mapping each entry's source path to its destination.
async fn read_dir_contents(src: PathBuf, dst: PathBuf) -> anyhow::Result<DirContents> {
    let mut files = Vec::new();
    let mut subdirs = Vec::new();
    let mut dir = fs::read_dir(&src).await?;

    while let Some(entry) = dir.next_entry().await? {
        let entry_path = entry.path();
        let file_type = entry.file_type().await?;

        if file_type.is_dir() {
            let dst_sub = dst.join(
                entry_path
                    .file_name()
                    .ok_or_else(|| anyhow::anyhow!("Could not read the subdirectory name"))?,
            );
            subdirs.push((entry_path, dst_sub));
        } else if file_type.is_file() {
            let dst_file = dst.join(
                entry_path
                    .file_name()
                    .ok_or_else(|| anyhow::anyhow!("Could not read the file name"))?,
            );
            files.push((entry_path, dst_file));
        }
    }

    Ok(DirContents { files, subdirs })
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
