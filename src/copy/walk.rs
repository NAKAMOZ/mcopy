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
