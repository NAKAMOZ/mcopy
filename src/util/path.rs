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

/// Repair a path mangled by Explorer's command-line quoting.
///
/// Context-menu verbs are registered as `"mcopy.exe" paste "%1"`. When the verb
/// targets a drive root, Explorer substitutes `C:\`, producing the command line
/// `"mcopy.exe" paste "C:\"`. `CommandLineToArgvW` then reads the `\"` as an
/// *escaped quote* rather than a terminator, so the argument mcopy receives is
/// the malformed `C:"` — which fails with a syntax error the moment anything
/// touches it. The same applies to the directory-background verb when the user
/// is browsing a drive root.
///
/// The registry side cannot express this unambiguously (the trailing separator
/// comes from Explorer, not from us), so the argument is repaired here. It is
/// safe to do so unconditionally: `"` is not a legal character in a Windows
/// path, so a trailing one is always this artifact and never real input.
///
/// Bare `C:` is then completed to `C:\`, because on Windows a drive letter with
/// no separator means "the current directory on that drive" rather than its
/// root — a subtle difference that would otherwise silently paste into the
/// wrong place.
pub fn repair_shell_argument(path: PathBuf) -> PathBuf {
    if !cfg!(windows) {
        return path;
    }

    let Some(text) = path.to_str() else {
        return path;
    };

    let trimmed = text.trim_end_matches('"');
    if trimmed == text {
        // No artifact, but a bare drive letter still needs its separator.
        return complete_drive_root(path);
    }

    complete_drive_root(PathBuf::from(trimmed))
}

/// Turn a bare `C:` into `C:\`.
fn complete_drive_root(path: PathBuf) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path;
    };

    let bytes = text.as_bytes();
    if bytes.len() == 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return PathBuf::from(format!("{text}\\"));
    }

    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrency_respects_an_explicit_override() {
        assert_eq!(calculate_concurrency(Some(8)), 8);
    }

    #[test]
    fn concurrency_never_drops_to_zero() {
        // A zero would stall `buffer_unordered` forever.
        assert_eq!(calculate_concurrency(Some(0)), 1);
    }

    #[test]
    fn concurrency_stays_within_bounds() {
        let resolved = calculate_concurrency(None);
        assert!((4..=128).contains(&resolved), "got {resolved}");
    }

    #[test]
    fn normalize_strips_the_unc_prefix() {
        assert_eq!(
            normalize_path(PathBuf::from(r"\\?\C:\Users\me")),
            PathBuf::from(r"C:\Users\me")
        );
    }

    #[test]
    fn normalize_leaves_ordinary_paths_untouched() {
        for path in [r"C:\Users\me", "/home/me", "relative/path"] {
            assert_eq!(
                normalize_path(PathBuf::from(path)),
                PathBuf::from(path)
            );
        }
    }

    #[test]
    fn normalize_preserves_a_unc_network_path() {
        // `\\server\share` is a real path, not the `\\?\` verbatim prefix.
        assert_eq!(
            normalize_path(PathBuf::from(r"\\server\share\dir")),
            PathBuf::from(r"\\server\share\dir")
        );
    }

    #[cfg(windows)]
    mod windows {
        use super::*;

        /// The exact argument Explorer delivers for a drive-root paste.
        #[test]
        fn repairs_a_drive_root_mangled_by_argv_quoting() {
            assert_eq!(
                repair_shell_argument(PathBuf::from(r#"C:""#)),
                PathBuf::from(r"C:\")
            );
        }

        #[test]
        fn repairs_a_directory_path_mangled_by_argv_quoting() {
            assert_eq!(
                repair_shell_argument(PathBuf::from(r#"C:\Users\me""#)),
                PathBuf::from(r"C:\Users\me")
            );
        }

        #[test]
        fn completes_a_bare_drive_letter_to_its_root() {
            // `C:` means "current directory on C:", which is not what the user
            // right-clicked on.
            assert_eq!(
                repair_shell_argument(PathBuf::from("C:")),
                PathBuf::from(r"C:\")
            );
            assert_eq!(
                repair_shell_argument(PathBuf::from("d:")),
                PathBuf::from(r"d:\")
            );
        }

        #[test]
        fn leaves_a_well_formed_path_untouched() {
            for path in [
                r"C:\",
                r"C:\Users\me",
                r"C:\Users\me\folder with spaces",
                r"\\server\share",
            ] {
                assert_eq!(
                    repair_shell_argument(PathBuf::from(path)),
                    PathBuf::from(path),
                    "{path} should pass through unchanged"
                );
            }
        }

        #[test]
        fn strips_repeated_trailing_quotes() {
            assert_eq!(
                repair_shell_argument(PathBuf::from(r#"C:\dir"""#)),
                PathBuf::from(r"C:\dir")
            );
        }

        /// A quote inside the path is not the artifact and must not be touched;
        /// only a trailing one is. (Such a path cannot exist on Windows, but the
        /// function must not corrupt input it does not understand.)
        #[test]
        fn leaves_interior_quotes_alone() {
            assert_eq!(
                repair_shell_argument(PathBuf::from(r#"C:\a"b"#)),
                PathBuf::from(r#"C:\a"b"#)
            );
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn shell_repair_is_a_no_op_off_windows() {
        // A quote is a legal filename character on Unix, so nothing is stripped.
        for path in ["/tmp/dir", r#"/tmp/we"ird"#, "/"] {
            assert_eq!(
                repair_shell_argument(PathBuf::from(path)),
                PathBuf::from(path)
            );
        }
    }
}
