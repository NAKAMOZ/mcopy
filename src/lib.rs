use futures::{StreamExt, stream};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;
use tokio::fs;

/// Kopyalama işi için kooperatif kontrol mekanizması.
///
/// Mevcut dosya kopyalama algoritmasını değiştirmeden, yeni işlerin başlatılmasını
/// durdurmak veya kalan kuyruğu iptal etmek için kullanılır.
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

/// İlerleme güncellemesi.
///
/// `processed_files`, başarılı veya hatalı tamamlanan dosya sayısını temsil eder.
#[derive(Clone, Debug)]
pub struct ProgressUpdate {
    pub phase: ProgressPhase,
    pub processed_files: usize,
    pub file_name: String,
    pub file_bytes: u64,
}

/// Progress callback türü.
pub type ProgressCallback = Box<dyn Fn(ProgressUpdate) + Send + Sync>;

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
    (cores * 4).clamp(4, 128)
}

/// Windows UNC path prefix'ini kaldır (\\?\C:\... -> C:\...)
pub fn normalize_path(path: PathBuf) -> PathBuf {
    let path_str = path.to_string_lossy();
    if let Some(stripped) = path_str.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path
    }
}

/// Dosyaları progress callback ile kopyala
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

            // Dosya adını başta belirle ki hata durumunda da UI güncellenebilsin.
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

            // Dosya boyutunu al
            let file_size = match fs::metadata(&src).await {
                Ok(meta) => meta.len(),
                Err(e) => {
                    eprintln!("HATA: {:?} metadata okunamadı | {}", src, e);

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

            // Dosyayı kopyala
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
                    eprintln!("HATA: {:?} -> {:?} | {}", src, dst, e);

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
