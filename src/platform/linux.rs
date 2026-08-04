//! File-manager integration for Nautilus (GNOME) and Dolphin (KDE).
//!
//! Both integrations are per-user files under `$HOME`, so nothing here ever
//! needs root.
//!
//! Thunar support was removed in 0.3. It was never actually installed: the
//! previous implementation only printed setup instructions to stdout, which is
//! discarded when mcopy runs from a file manager, so no user ever saw them.
//! Editing Thunar's `uca.xml` in place is the only real option and is not worth
//! the risk of corrupting a user's action list; Thunar is documented as
//! unsupported instead.

use super::{ContextMenu, ContextMenuInstallState, PasteVisibility};
use crate::log_info;
use crate::platform::state::CURRENT_VERSION;
use crate::util::shell::{
    escape_desktop_value, quote_desktop_exec, quote_posix,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const SUPPORT_DIR: &str = ".local/share/mcopy";
const VERSION_FILE: &str = "install-version";
const EXE_PATH_FILE: &str = "install-exe";

const NAUTILUS_SCRIPTS_DIR: &str = ".local/share/nautilus/scripts";
const NAUTILUS_COPY_SCRIPT: &str = "mcopy-copy";
const NAUTILUS_PASTE_SCRIPT: &str = "mcopy-paste";

/// KDE service-menu directories, relative to `$HOME`.
///
/// Plasma 5 read from `kservices5/ServiceMenus`; Plasma 6 moved to
/// `kio/servicemenus`. Installing to both keeps Dolphin working on either
/// version (the unused path is harmless).
const DOLPHIN_SERVICE_DIRS: [&str; 2] = [
    ".local/share/kservices5/ServiceMenus",
    ".local/share/kio/servicemenus",
];
const DOLPHIN_SERVICE_FILE: &str = "mcopy.desktop";

pub struct LinuxMenu;

impl ContextMenu for LinuxMenu {
    fn install(exe_path: &Path) -> anyhow::Result<()> {
        let home = home_dir()?;
        let exe_str = exe_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("The executable path is invalid"))?;

        install_nautilus_copy_script(&home, exe_str)?;
        // A fresh install has nothing to paste yet, so the paste affordances
        // start hidden and appear on the first copy.
        write_dolphin_service(&home, exe_str, false)?;
        set_nautilus_paste_present(&home, exe_str, false)?;

        write_install_metadata(&home, exe_str)?;

        log_info!("installed Nautilus and Dolphin integration");
        Ok(())
    }

    fn uninstall() -> anyhow::Result<()> {
        let home = home_dir()?;

        let scripts_dir = PathBuf::from(&home).join(NAUTILUS_SCRIPTS_DIR);
        let _ = fs::remove_file(scripts_dir.join(NAUTILUS_COPY_SCRIPT));
        let _ = fs::remove_file(scripts_dir.join(NAUTILUS_PASTE_SCRIPT));

        for dir in DOLPHIN_SERVICE_DIRS {
            let _ = fs::remove_file(
                PathBuf::from(&home).join(dir).join(DOLPHIN_SERVICE_FILE),
            );
        }

        remove_install_metadata(&home);

        log_info!("removed Nautilus and Dolphin integration");
        Ok(())
    }

    fn state() -> anyhow::Result<ContextMenuInstallState> {
        let home = home_dir()?;

        if let Ok(version) = fs::read_to_string(version_file_path(&home)) {
            let version = version.trim().to_string();
            if !version.is_empty() {
                return Ok(ContextMenuInstallState::Installed {
                    version: Some(version),
                });
            }
        }

        let scripts_dir = PathBuf::from(&home).join(NAUTILUS_SCRIPTS_DIR);
        let dolphin_installed = DOLPHIN_SERVICE_DIRS.iter().any(|dir| {
            PathBuf::from(&home)
                .join(dir)
                .join(DOLPHIN_SERVICE_FILE)
                .exists()
        });

        if scripts_dir.join(NAUTILUS_COPY_SCRIPT).exists()
            || scripts_dir.join(NAUTILUS_PASTE_SCRIPT).exists()
            || dolphin_installed
        {
            return Ok(ContextMenuInstallState::Installed { version: None });
        }

        Ok(ContextMenuInstallState::NotInstalled)
    }

    /// Show or hide the paste affordances.
    ///
    /// Neither file manager can evaluate a condition at menu-build time, so
    /// visibility is expressed by the presence of the script and by which
    /// actions the Dolphin service file declares. Both are re-read every time
    /// the menu opens, so the change takes effect immediately.
    fn set_paste_available(available: bool) -> anyhow::Result<PasteVisibility> {
        let home = home_dir()?;

        // Nothing installed yet: there is no menu to gate.
        let Some(exe) = read_installed_exe(&home) else {
            return Ok(PasteVisibility::Unsupported);
        };

        set_nautilus_paste_present(&home, &exe, available)?;
        write_dolphin_service(&home, &exe, available)?;

        Ok(PasteVisibility::Applied)
    }
}

/// Resolve the user's home directory via `dirs` (handles edge cases more
/// robustly than reading `$HOME`, and matches how Windows resolves paths).
fn home_dir() -> anyhow::Result<String> {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| {
            anyhow::anyhow!("Could not determine the home directory")
        })
}

fn write_install_metadata(home: &str, exe_path: &str) -> anyhow::Result<()> {
    let support_dir = PathBuf::from(home).join(SUPPORT_DIR);
    fs::create_dir_all(&support_dir)?;
    fs::write(support_dir.join(VERSION_FILE), CURRENT_VERSION)?;
    // Remembered so paste gating can rewrite the menu files later without
    // depending on where the toggling process happens to be running from.
    fs::write(support_dir.join(EXE_PATH_FILE), exe_path)?;
    Ok(())
}

fn remove_install_metadata(home: &str) {
    let _ = fs::remove_file(version_file_path(home));
    let _ = fs::remove_file(exe_path_file_path(home));
}

fn version_file_path(home: &str) -> PathBuf {
    PathBuf::from(home).join(SUPPORT_DIR).join(VERSION_FILE)
}

fn exe_path_file_path(home: &str) -> PathBuf {
    PathBuf::from(home).join(SUPPORT_DIR).join(EXE_PATH_FILE)
}

fn read_installed_exe(home: &str) -> Option<String> {
    fs::read_to_string(exe_path_file_path(home))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

// Note: Nautilus 43+ de-emphasizes `~/.local/share/nautilus/scripts` in favor
// of `nautilus-python` extensions. Scripts still run but are less discoverable;
// a proper extension could be added later if discoverability becomes a problem.
fn install_nautilus_copy_script(
    home: &str,
    exe_path: &str,
) -> anyhow::Result<()> {
    let scripts_dir = PathBuf::from(home).join(NAUTILUS_SCRIPTS_DIR);
    fs::create_dir_all(&scripts_dir)?;

    // `quote_posix` keeps a path containing spaces, quotes or `$` as a single
    // inert word; 0.2 interpolated it bare inside double quotes.
    let script = format!(
        "#!/bin/sh\n\
         # mcopy - copy the selected files and folders\n\
         set -eu\n\
         for arg in \"$@\"; do\n\
         \t{exe} copy --append \"$arg\"\n\
         done\n",
        exe = quote_posix(exe_path)
    );

    write_executable(&scripts_dir.join(NAUTILUS_COPY_SCRIPT), &script)
}

/// Create or remove the Nautilus paste script.
///
/// Nautilus lists every executable file in the scripts directory, so removing
/// the file is the only way to hide the entry.
fn set_nautilus_paste_present(
    home: &str,
    exe_path: &str,
    present: bool,
) -> anyhow::Result<()> {
    let scripts_dir = PathBuf::from(home).join(NAUTILUS_SCRIPTS_DIR);
    let script_path = scripts_dir.join(NAUTILUS_PASTE_SCRIPT);

    if !present {
        match fs::remove_file(&script_path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(anyhow::anyhow!(
                "could not hide the Nautilus paste script: {e}"
            )),
        }
    } else {
        fs::create_dir_all(&scripts_dir)?;
        let script = format!(
            "#!/bin/sh\n\
             # mcopy - paste into the current directory\n\
             set -eu\n\
             {exe} paste \"${{NAUTILUS_SCRIPT_CURRENT_URI:-$PWD}}\"\n",
            exe = quote_posix(exe_path)
        );
        write_executable(&script_path, &script)
    }
}

fn write_executable(path: &Path, contents: &str) -> anyhow::Result<()> {
    fs::write(path, contents)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

/// Write the Dolphin service menu, optionally including the paste action.
///
/// Dolphin has no conditional-visibility mechanism for service menus, so the
/// paste action is simply absent from `Actions=` when there is nothing to paste.
fn write_dolphin_service(
    home: &str,
    exe_path: &str,
    include_paste: bool,
) -> anyhow::Result<()> {
    let exe = quote_desktop_exec(exe_path);
    let actions = if include_paste {
        "mcopy_copy;mcopy_paste;"
    } else {
        "mcopy_copy;"
    };

    let mut entry = format!(
        "[Desktop Entry]\n\
         Type=Service\n\
         ServiceTypes=KonqPopupMenu/Plugin\n\
         MimeType=all/all;\n\
         Actions={actions}\n\
         X-KDE-Priority=TopLevel\n\
         \n\
         [Desktop Action mcopy_copy]\n\
         Name={copy_name}\n\
         Icon=edit-copy\n\
         Exec={exe} copy --append %F\n",
        copy_name = escape_desktop_value("Copy with mcopy"),
    );

    if include_paste {
        entry.push_str(&format!(
            "\n\
             [Desktop Action mcopy_paste]\n\
             Name={paste_name}\n\
             Icon=edit-paste\n\
             Exec={exe} paste %d\n",
            paste_name = escape_desktop_value("Paste with mcopy"),
        ));
    }

    for dir in DOLPHIN_SERVICE_DIRS {
        let services_dir = PathBuf::from(home).join(dir);
        fs::create_dir_all(&services_dir)?;
        fs::write(services_dir.join(DOLPHIN_SERVICE_FILE), &entry)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOSTILE_EXE: &str = "/home/u/My Apps/mcopy$x\"y";

    #[test]
    fn the_dolphin_service_omits_paste_when_nothing_is_copied() {
        let dir = tempdir();
        write_dolphin_service(path_str(&dir), "/usr/bin/mcopy", false).unwrap();

        let entry = read_dolphin_service(&dir);
        assert!(entry.contains("Actions=mcopy_copy;\n"));
        assert!(!entry.contains("[Desktop Action mcopy_paste]"));
    }

    #[test]
    fn the_dolphin_service_includes_paste_once_something_is_copied() {
        let dir = tempdir();
        write_dolphin_service(path_str(&dir), "/usr/bin/mcopy", true).unwrap();

        let entry = read_dolphin_service(&dir);
        assert!(entry.contains("Actions=mcopy_copy;mcopy_paste;\n"));
        assert!(entry.contains("[Desktop Action mcopy_paste]"));
        assert!(entry.contains("Exec=\"/usr/bin/mcopy\" paste %d"));
    }

    #[test]
    fn the_dolphin_service_is_written_to_both_plasma_locations() {
        let dir = tempdir();
        write_dolphin_service(path_str(&dir), "/usr/bin/mcopy", true).unwrap();

        for service_dir in DOLPHIN_SERVICE_DIRS {
            assert!(
                dir.path()
                    .join(service_dir)
                    .join(DOLPHIN_SERVICE_FILE)
                    .exists(),
                "{service_dir} was not written"
            );
        }
    }

    #[test]
    fn the_dolphin_exec_line_quotes_a_hostile_path() {
        let dir = tempdir();
        write_dolphin_service(path_str(&dir), HOSTILE_EXE, true).unwrap();

        let entry = read_dolphin_service(&dir);
        // Reserved Exec characters must be escaped, and the path must stay one
        // argument despite the space.
        assert!(entry.contains(r#"Exec="/home/u/My Apps/mcopy\\$x\\"y" copy"#));
    }

    #[test]
    fn the_nautilus_paste_script_appears_and_disappears() {
        let dir = tempdir();
        let home = path_str(&dir);
        let script = dir
            .path()
            .join(NAUTILUS_SCRIPTS_DIR)
            .join(NAUTILUS_PASTE_SCRIPT);

        set_nautilus_paste_present(home, "/usr/bin/mcopy", true).unwrap();
        assert!(script.exists(), "paste script should be installed");

        set_nautilus_paste_present(home, "/usr/bin/mcopy", false).unwrap();
        assert!(!script.exists(), "paste script should be removed");
    }

    #[test]
    fn hiding_an_absent_paste_script_is_not_an_error() {
        let dir = tempdir();
        // No install has happened; hiding must still succeed.
        set_nautilus_paste_present(path_str(&dir), "/usr/bin/mcopy", false)
            .unwrap();
    }

    #[test]
    fn nautilus_scripts_are_executable() {
        let dir = tempdir();
        install_nautilus_copy_script(path_str(&dir), "/usr/bin/mcopy").unwrap();

        let script = dir
            .path()
            .join(NAUTILUS_SCRIPTS_DIR)
            .join(NAUTILUS_COPY_SCRIPT);
        let mode = fs::metadata(&script).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "script must be executable");
    }

    #[test]
    fn nautilus_scripts_quote_a_hostile_path() {
        let dir = tempdir();
        install_nautilus_copy_script(path_str(&dir), HOSTILE_EXE).unwrap();

        let script = fs::read_to_string(
            dir.path()
                .join(NAUTILUS_SCRIPTS_DIR)
                .join(NAUTILUS_COPY_SCRIPT),
        )
        .unwrap();

        // Single-quoted, so `$x` cannot expand and `"` cannot end a word.
        assert!(
            script.contains(r#"'/home/u/My Apps/mcopy$x"y' copy --append"#)
        );
    }

    #[test]
    fn the_installed_exe_path_round_trips() {
        let dir = tempdir();
        let home = path_str(&dir);

        assert_eq!(read_installed_exe(home), None);
        write_install_metadata(home, HOSTILE_EXE).unwrap();
        assert_eq!(read_installed_exe(home).as_deref(), Some(HOSTILE_EXE));

        remove_install_metadata(home);
        assert_eq!(read_installed_exe(home), None);
    }

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("could not create a temporary directory")
    }

    fn path_str(dir: &tempfile::TempDir) -> &str {
        dir.path().to_str().expect("temp path must be UTF-8")
    }

    fn read_dolphin_service(dir: &tempfile::TempDir) -> String {
        fs::read_to_string(
            dir.path()
                .join(DOLPHIN_SERVICE_DIRS[0])
                .join(DOLPHIN_SERVICE_FILE),
        )
        .expect("service file should exist")
    }
}
