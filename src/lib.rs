use futures::{StreamExt, stream};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Progress callback türü
/// (current_file_idx, total_files, filename, bytes_copied, total_bytes)
pub type ProgressCallback = Box<dyn Fn(usize, usize, String, u64, u64) + Send + Sync>;

/// Dosyaları recursive olarak topla (hem dosya hem klasör destekler)
pub async fn collect_files(
    src_root: &Path,
    dst_root: &Path,
) -> anyhow::Result<Vec<(PathBuf, PathBuf)>> {
    // Kaynak dosya mı klasör mü kontrol et
    let metadata = fs::metadata(src_root).await?;

    // Tek dosya ise direkt kopyala
    if metadata.is_file() {
        let file_name = src_root
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Dosya ismi okunamadı"))?;
        let dst_path = dst_root.join(file_name);
        return Ok(vec![(src_root.to_path_buf(), dst_path)]);
    }

    // Klasör ise recursive olarak topla
    let mut result = Vec::new();
    let mut stack: Vec<(PathBuf, PathBuf)> = Vec::new();

    // Kaynak root klasörün ismini al
    // Disk kökü (örn: D:\) için file_name() None döner, bu durumda doğrudan dst_root kullan
    let initial_dst = match src_root.file_name() {
        Some(src_name) => dst_root.join(src_name),
        None => dst_root.to_path_buf(), // Disk kökü - doğrudan hedef klasöre kopyala
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
                    .ok_or_else(|| anyhow::anyhow!("Alt klasör ismi okunamadı"))?;
                let dst_sub = current_dst.join(name);
                stack.push((entry_path, dst_sub));
            } else if file_type.is_file() {
                let name = entry_path
                    .file_name()
                    .ok_or_else(|| anyhow::anyhow!("Dosya ismi okunamadı"))?;
                let dst_file_path = current_dst.join(name);
                result.push((entry_path, dst_file_path));
            }
        }
    }

    Ok(result)
}

/// Hedef klasörleri önceden oluştur (optimizasyon)
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

/// Optimal concurrency hesapla
pub fn calculate_concurrency(user_override: Option<usize>) -> usize {
    if let Some(n) = user_override {
        return n.max(1);
    }
    let cores = num_cpus::get();
    (cores * 4).min(128).max(4)
}

/// Dosyaları progress callback ile kopyala
pub async fn copy_files_with_progress(
    files: Vec<(PathBuf, PathBuf)>,
    concurrency: usize,
    callback: Option<ProgressCallback>,
) -> anyhow::Result<()> {
    let total_files = files.len();
    let files_processed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

    stream::iter(files.into_iter().enumerate().map(|(idx, (src, dst))| {
        let callback = callback.as_ref().map(|cb| cb as &ProgressCallback);
        let files_processed = files_processed.clone();

        async move {
            // Dosya boyutunu al
            let file_size = match fs::metadata(&src).await {
                Ok(meta) => meta.len(),
                Err(e) => {
                    eprintln!("HATA: {:?} metadata okunamadı | {}", src, e);
                    return;
                }
            };

            // Dosya adını al
            let file_name = src
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            // Callback çağır (başlangıç)
            if let Some(cb) = callback {
                cb(idx + 1, total_files, file_name.clone(), 0, file_size);
            }

            // Dosyayı kopyala
            match fs::copy(&src, &dst).await {
                Ok(_) => {
                    files_processed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

                    // Callback çağır (tamamlandı)
                    if let Some(cb) = callback {
                        cb(idx + 1, total_files, file_name, file_size, file_size);
                    }
                }
                Err(e) => {
                    eprintln!("HATA: {:?} -> {:?} | {}", src, dst, e);
                }
            }
        }
    }))
    .buffer_unordered(concurrency)
    .for_each(|_| async {})
    .await;

    Ok(())
}
