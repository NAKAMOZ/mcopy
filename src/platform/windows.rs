//! Explorer context-menu integration.
//!
//! # Why HKCU
//!
//! Version 0.2 wrote these keys under `HKEY_LOCAL_MACHINE\SOFTWARE\Classes`,
//! which requires administrator rights. That is the root of the reported
//! "permissions granted at install time stop working" problem: UAC elevation is
//! a property of a *process token*, not a durable grant, so the later
//! non-elevated run had no way to touch the keys it had created — and the
//! failure surfaced as an opaque exit code from a hidden child process.
//!
//! Per-user shell verbs belong in `HKEY_CURRENT_USER\Software\Classes`, which
//! Explorer merges into `HKEY_CLASSES_ROOT` exactly the same way and which the
//! user can always write. The entire elevation path therefore disappears rather
//! than being made more robust.

use super::{ContextMenu, ContextMenuInstallState, PasteVisibility};
use crate::platform::state::CURRENT_VERSION;
use crate::{log_info, log_warn};
use std::path::Path;
use winreg::RegKey;
use winreg::enums::*;

const VERSION_VALUE: &str = "mcopyVersion";
const EXE_PATH_VALUE: &str = "mcopyExePath";

/// Explorer hides a verb whose key carries this value, whatever its contents.
///
/// This is the only documented way to toggle a static verb's visibility without
/// shipping a COM `IExplorerCommand` handler. Explorer re-reads it every time
/// the menu is built, so the change is immediate and needs no restart.
const LEGACY_DISABLE_VALUE: &str = "LegacyDisable";

/// One Explorer context-menu entry. The five registry installers used to be
/// near-identical functions; they are now rows in this single table, which also
/// feeds uninstall and the install-state probe so the paths never drift.
struct MenuEntry {
    /// Key path relative to the classes root.
    path: &'static str,
    /// Menu label shown in Explorer.
    label: &'static str,
    /// Command line; `{exe}` is replaced with the executable path.
    command_template: &'static str,
    /// Explorer should invoke the command once per selected item.
    multi_select: bool,
    /// Whether this entry is one of the paste verbs, and so participates in
    /// copy-state gating.
    is_paste: bool,
}

const MENU_ENTRIES: &[MenuEntry] = &[
    // "Copy with mcopy" for files.
    MenuEntry {
        path: r"Software\Classes\*\shell\mcopy_copy",
        label: "Copy with mcopy",
        command_template: r#""{exe}" copy --append "%1""#,
        multi_select: true,
        is_paste: false,
    },
    // "Copy with mcopy" for directories.
    MenuEntry {
        path: r"Software\Classes\Directory\shell\mcopy_copy",
        label: "Copy with mcopy",
        command_template: r#""{exe}" copy --append "%1""#,
        multi_select: true,
        is_paste: false,
    },
    // "Paste with mcopy" on the directory background.
    MenuEntry {
        path: r"Software\Classes\Directory\Background\shell\mcopy_paste",
        label: "Paste with mcopy",
        command_template: r#""{exe}" paste "%V""#,
        multi_select: false,
        is_paste: true,
    },
    // "Paste here with mcopy" on a directory entry.
    MenuEntry {
        path: r"Software\Classes\Directory\shell\mcopy_paste",
        label: "Paste here with mcopy",
        command_template: r#""{exe}" paste "%1""#,
        multi_select: false,
        is_paste: true,
    },
    // "Paste with mcopy" for drive roots such as D:\ or E:\.
    MenuEntry {
        path: r"Software\Classes\Drive\shell\mcopy_paste",
        label: "Paste with mcopy",
        command_template: r#""{exe}" paste "%1""#,
        multi_select: false,
        is_paste: true,
    },
];

/// The entry whose version we trust as the authoritative install marker.
const PRIMARY_MENU_PATH: &str =
    r"Software\Classes\Directory\Background\shell\mcopy_paste";

/// 0.2 installed these under HKLM. They are removed on upgrade when possible.
const LEGACY_HKLM_PATHS: &[&str] = &[
    r"SOFTWARE\Classes\*\shell\mcopy_copy",
    r"SOFTWARE\Classes\Directory\shell\mcopy_copy",
    r"SOFTWARE\Classes\Directory\Background\shell\mcopy_paste",
    r"SOFTWARE\Classes\Directory\shell\mcopy_paste",
    r"SOFTWARE\Classes\Drive\shell\mcopy_paste",
];

pub struct WindowsMenu;

impl ContextMenu for WindowsMenu {
    fn install(exe_path: &Path) -> anyhow::Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let exe_str = exe_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("The executable path is invalid"))?;

        for entry in MENU_ENTRIES {
            install_entry(&hkcu, exe_str, entry)?;
        }

        // A fresh install has no copy state yet, so the paste verbs start
        // hidden and only appear once something has actually been copied.
        set_paste_hidden(&hkcu, true)?;

        remove_legacy_hklm_entries();

        log_info!("registered {} menu entries in HKCU", MENU_ENTRIES.len());
        Ok(())
    }

    fn uninstall() -> anyhow::Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);

        for entry in MENU_ENTRIES {
            delete_entry(&hkcu, entry)?;
        }

        remove_legacy_hklm_entries();

        log_info!("removed menu entries from HKCU");
        Ok(())
    }

    fn state() -> anyhow::Result<ContextMenuInstallState> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);

        if let Ok(key) =
            hkcu.open_subkey_with_flags(PRIMARY_MENU_PATH, KEY_READ)
        {
            return Ok(ContextMenuInstallState::Installed {
                version: read_version(&key),
            });
        }

        for entry in MENU_ENTRIES {
            if hkcu.open_subkey_with_flags(entry.path, KEY_READ).is_ok() {
                return Ok(ContextMenuInstallState::Installed {
                    version: None,
                });
            }
        }

        Ok(ContextMenuInstallState::NotInstalled)
    }

    fn set_paste_available(available: bool) -> anyhow::Result<PasteVisibility> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        set_paste_hidden(&hkcu, !available)?;
        Ok(PasteVisibility::Applied)
    }
}

/// Add or remove `LegacyDisable` on every paste verb.
///
/// Missing keys are skipped rather than treated as an error: the integration
/// may simply not be installed, and copy/paste must keep working regardless.
fn set_paste_hidden(root: &RegKey, hidden: bool) -> anyhow::Result<()> {
    for entry in MENU_ENTRIES.iter().filter(|entry| entry.is_paste) {
        let Ok(key) = root.open_subkey_with_flags(entry.path, KEY_SET_VALUE)
        else {
            continue;
        };

        let result = if hidden {
            key.set_value(LEGACY_DISABLE_VALUE, &"")
        } else {
            match key.delete_value(LEGACY_DISABLE_VALUE) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            }
        };

        result.map_err(|e| {
            anyhow::anyhow!(
                "could not update the paste menu entry at {}: {}",
                entry.path,
                e
            )
        })?;
    }

    Ok(())
}

/// Best-effort cleanup of the 0.2 machine-wide keys.
///
/// Writing to HKLM needs elevation the 0.3 flow deliberately never requests, so
/// this succeeds only when mcopy happens to be running elevated. Leftovers are
/// harmless — HKCU entries take precedence in the merged view — but they are
/// logged so `mcopy shell-uninstall --all-users` can be recommended.
fn remove_legacy_hklm_entries() {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut remaining = Vec::new();

    for path in LEGACY_HKLM_PATHS {
        let Some((base_path, menu_name)) = path.rsplit_once('\\') else {
            continue;
        };

        match hklm.open_subkey_with_flags(base_path, KEY_WRITE) {
            Ok(key) => match key.delete_subkey_all(menu_name) {
                Ok(()) => log_info!("removed legacy HKLM entry {path}"),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
                Err(_) => remaining.push(*path),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
            Err(_) => remaining.push(*path),
        }
    }

    if !remaining.is_empty() {
        log_warn!(
            "{} legacy machine-wide entries from mcopy 0.2 remain; run \
             `mcopy shell-uninstall --all-users` from an elevated prompt to \
             remove them",
            remaining.len()
        );
    }
}

/// Whether any 0.2 machine-wide entry is still present.
pub fn legacy_hklm_entries_present() -> bool {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    LEGACY_HKLM_PATHS
        .iter()
        .any(|path| hklm.open_subkey_with_flags(path, KEY_READ).is_ok())
}

/// Remove the 0.2 machine-wide entries, reporting failures.
///
/// Only reachable through the explicit `--all-users` CLI path, which is the one
/// place where asking for administrator rights is justified.
pub fn uninstall_all_users() -> anyhow::Result<()> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    for path in LEGACY_HKLM_PATHS {
        let (base_path, menu_name) = path
            .rsplit_once('\\')
            .ok_or_else(|| anyhow::anyhow!("Invalid menu path: {path}"))?;

        match hklm.open_subkey_with_flags(base_path, KEY_WRITE) {
            Ok(key) => match key.delete_subkey_all(menu_name) {
                Ok(()) => {},
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    anyhow::bail!(
                        "Administrator rights are required to remove the \
                         machine-wide entry at {path}."
                    )
                },
                Err(e) => anyhow::bail!("could not remove {path}: {e}"),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                anyhow::bail!(
                    "Administrator rights are required to remove the \
                     machine-wide entry at {path}."
                )
            },
            Err(e) => anyhow::bail!("could not open {path}: {e}"),
        }
    }

    Ok(())
}

/// Create one menu entry plus its `command` subkey.
fn install_entry(
    root: &RegKey,
    exe_path: &str,
    entry: &MenuEntry,
) -> anyhow::Result<()> {
    let (key, _) = root.create_subkey(entry.path)?;
    key.set_value("", &entry.label)?;
    write_metadata(&key, exe_path)?;

    // Explorer invokes the command once per selected item; `--append`
    // lets every invocation extend the shared clipboard session.
    if entry.multi_select {
        key.set_value("MultiSelectModel", &"Player")?;
    }

    let (cmd_key, _) =
        root.create_subkey(format!("{}\\command", entry.path))?;
    let command = entry.command_template.replace("{exe}", exe_path);
    cmd_key.set_value("", &command)?;

    Ok(())
}

/// Delete one menu entry, splitting its path into base key + entry name.
fn delete_entry(root: &RegKey, entry: &MenuEntry) -> anyhow::Result<()> {
    let (base_path, menu_name) = entry
        .path
        .rsplit_once('\\')
        .ok_or_else(|| anyhow::anyhow!("Invalid menu path: {}", entry.path))?;
    delete_menu_entry(root, base_path, menu_name)
}

fn write_metadata(key: &RegKey, exe_path: &str) -> anyhow::Result<()> {
    key.set_value(VERSION_VALUE, &CURRENT_VERSION)?;
    key.set_value(EXE_PATH_VALUE, &exe_path)?;
    key.set_value("Icon", &format!("\"{}\",0", exe_path))?;
    Ok(())
}

fn read_version(key: &RegKey) -> Option<String> {
    key.get_value::<String, _>(VERSION_VALUE)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// Delete a single menu entry if it exists.
fn delete_menu_entry(
    root: &RegKey,
    base_path: &str,
    menu_name: &str,
) -> anyhow::Result<()> {
    match root.open_subkey_with_flags(base_path, KEY_WRITE) {
        Ok(key) => match key.delete_subkey_all(menu_name) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(anyhow::anyhow!("Registry delete error: {}", e)),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(anyhow::anyhow!("Registry open error: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression guard for issue 5: any HKLM path in the active table would
    /// reintroduce the elevation requirement this release removes.
    #[test]
    fn every_active_entry_lives_under_the_per_user_classes_root() {
        for entry in MENU_ENTRIES {
            assert!(
                entry.path.starts_with(r"Software\Classes\"),
                "{} is not under the per-user classes root",
                entry.path
            );
            assert!(
                !entry.path.starts_with("SOFTWARE\\Classes\\"),
                "{} uses the 0.2 machine-wide casing",
                entry.path
            );
        }
    }

    #[test]
    fn the_primary_marker_is_one_of_the_installed_entries() {
        assert!(
            MENU_ENTRIES
                .iter()
                .any(|entry| entry.path == PRIMARY_MENU_PATH),
            "the install-state marker must be a key we actually create"
        );
    }

    #[test]
    fn exactly_the_paste_verbs_are_gated() {
        let gated: Vec<_> = MENU_ENTRIES
            .iter()
            .filter(|entry| entry.is_paste)
            .map(|entry| entry.path)
            .collect();

        assert_eq!(gated.len(), 3, "all three paste verbs must be gated");
        assert!(gated.iter().all(|path| path.ends_with("mcopy_paste")));

        // Copy must never be gated: it is what creates the state in the first
        // place, and Explorer already scopes it to files and folders.
        assert!(
            MENU_ENTRIES
                .iter()
                .filter(|entry| !entry.is_paste)
                .all(|entry| entry.path.ends_with("mcopy_copy"))
        );
    }

    #[test]
    fn every_command_template_carries_the_exe_placeholder() {
        for entry in MENU_ENTRIES {
            assert!(
                entry.command_template.contains("{exe}"),
                "{} has no executable placeholder",
                entry.path
            );
            // The path is quoted so a `Program Files` install still parses as
            // one argument.
            assert!(
                entry.command_template.starts_with(r#""{exe}""#),
                "{} does not quote the executable path",
                entry.path
            );
        }
    }

    #[test]
    fn only_copy_entries_opt_into_multi_select() {
        for entry in MENU_ENTRIES {
            assert_eq!(
                entry.multi_select, !entry.is_paste,
                "{} has the wrong multi-select setting",
                entry.path
            );
        }
    }

    #[test]
    fn legacy_cleanup_covers_every_entry_the_previous_version_created() {
        assert_eq!(LEGACY_HKLM_PATHS.len(), MENU_ENTRIES.len());
        for path in LEGACY_HKLM_PATHS {
            assert!(path.starts_with(r"SOFTWARE\Classes\"));
        }
    }
}
