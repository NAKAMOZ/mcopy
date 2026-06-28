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
    let path_str = path.to_string_lossy();
    if let Some(stripped) = path_str.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path
    }
}
