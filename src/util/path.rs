use std::path::PathBuf;

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
    // Fast path: only convert to a string when the prefix is actually present,
    // so the common (unprefixed) case allocates nothing.
    if !path.as_os_str().as_encoded_bytes().starts_with(br"\\?\") {
        return path;
    }

    match path.to_string_lossy().strip_prefix(r"\\?\") {
        Some(stripped) => PathBuf::from(stripped),
        None => path,
    }
}
