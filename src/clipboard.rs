use arboard::Clipboard;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Timestamp dosyasının yolu
fn get_timestamp_path() -> PathBuf {
    std::env::temp_dir().join("mcopy_session.tmp")
}

/// Son kopyalama zamanını oku (epoch saniye)
fn get_last_copy_time() -> Option<u64> {
    std::fs::read_to_string(get_timestamp_path())
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Son kopyalama zamanını kaydet
fn set_last_copy_time() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let _ = std::fs::write(get_timestamp_path(), now.to_string());
}

/// Şu anki zaman (epoch saniye)
fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Windows UNC path prefix'ini kaldır (\\?\C:\... -> C:\...)
fn normalize_path(path: PathBuf) -> PathBuf {
    let path_str = path.to_string_lossy();
    if path_str.starts_with(r"\\?\") {
        PathBuf::from(&path_str[4..])
    } else {
        path
    }
}

/// Paths'i clipboard'a yaz (newline separated, absolute paths)
pub fn copy_paths_to_clipboard(paths: &[PathBuf]) -> anyhow::Result<()> {
    let mut clipboard = Clipboard::new()?;

    // Absolute path'lere çevir ve normalize et
    let abs_paths: Vec<String> = paths
        .iter()
        .filter_map(|p| {
            p.canonicalize()
                .ok()
                .map(normalize_path)
                .and_then(|abs| abs.to_str().map(|s| s.to_string()))
        })
        .collect();

    if abs_paths.is_empty() {
        anyhow::bail!("Kopyalanacak geçerli dosya yolu bulunamadı");
    }

    let text = abs_paths.join("\n");
    clipboard.set_text(text)?;
    set_last_copy_time();
    Ok(())
}

/// Mevcut clipboard'a path ekle (çoklu seçim için)
/// Eğer son kopyalamadan 2 saniyeden fazla geçtiyse, yeni session başlat
pub fn append_paths_to_clipboard(paths: &[PathBuf]) -> anyhow::Result<()> {
    const SESSION_TIMEOUT_SECS: u64 = 2;

    // Son kopyalama zamanını kontrol et
    let should_clear = match get_last_copy_time() {
        Some(last_time) => now_epoch() - last_time > SESSION_TIMEOUT_SECS,
        None => true, // İlk kez çalışıyor
    };

    // Mevcut path'leri oku (timeout geçtiyse veya hata varsa boş liste)
    let mut existing = if should_clear {
        Vec::new()
    } else {
        paste_paths_from_clipboard().unwrap_or_default()
    };

    // Yeni path'leri ekle (duplicate kontrolü ile)
    for path in paths {
        if let Ok(abs_path) = path.canonicalize().map(normalize_path) {
            if !existing.contains(&abs_path) {
                existing.push(abs_path);
            }
        }
    }

    if existing.is_empty() {
        anyhow::bail!("Eklenecek geçerli dosya yolu bulunamadı");
    }

    // Clipboard'a yaz
    let mut clipboard = Clipboard::new()?;
    let text = existing
        .iter()
        .filter_map(|p| p.to_str())
        .collect::<Vec<_>>()
        .join("\n");

    clipboard.set_text(text)?;
    set_last_copy_time();
    Ok(())
}

/// Clipboard'tan paths oku (newline split, validation ile)
pub fn paste_paths_from_clipboard() -> anyhow::Result<Vec<PathBuf>> {
    let mut clipboard = Clipboard::new()?;
    let text = clipboard.get_text()?;

    if text.trim().is_empty() {
        return Ok(Vec::new());
    }

    let paths: Vec<PathBuf> = text
        .lines()
        .map(|line| PathBuf::from(line.trim()))
        .filter(|p| p.exists())
        .collect();

    Ok(paths)
}

/// Clipboard'ı temizle
pub fn clear_clipboard() -> anyhow::Result<()> {
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text("")?;
    // Timestamp dosyasını da sil
    let _ = std::fs::remove_file(get_timestamp_path());
    Ok(())
}
