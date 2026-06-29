use super::{ContextMenu, ContextMenuInstallState};
use crate::platform::state::CURRENT_VERSION;
use std::path::Path;
use winreg::RegKey;
use winreg::enums::*;

const VERSION_VALUE: &str = "mcopyVersion";
const EXE_PATH_VALUE: &str = "mcopyExePath";

/// One Explorer context-menu entry. The five registry installers used to be
/// near-identical functions; they are now rows in this single table, which also
/// feeds uninstall and the install-state probe so the paths never drift.
struct MenuEntry {
    /// Full key path under HKEY_LOCAL_MACHINE.
    path: &'static str,
    /// Menu label shown in Explorer.
    label: &'static str,
    /// Command line; `{exe}` is replaced with the executable path.
    command_template: &'static str,
    /// Explorer should invoke the command once per selected item.
    multi_select: bool,
}

const MENU_ENTRIES: &[MenuEntry] = &[
    // "Copy with mcopy" for files.
    MenuEntry {
        path: r"SOFTWARE\Classes\*\shell\mcopy_copy",
        label: "Copy with mcopy",
        command_template: r#""{exe}" copy --append "%1""#,
        multi_select: true,
    },
    // "Copy with mcopy" for directories.
    MenuEntry {
        path: r"SOFTWARE\Classes\Directory\shell\mcopy_copy",
        label: "Copy with mcopy",
        command_template: r#""{exe}" copy --append "%1""#,
        multi_select: true,
    },
    // "Paste with mcopy" on the directory background.
    MenuEntry {
        path: r"SOFTWARE\Classes\Directory\Background\shell\mcopy_paste",
        label: "Paste with mcopy",
        command_template: r#""{exe}" paste "%V""#,
        multi_select: false,
    },
    // "Paste here with mcopy" on a directory entry.
    MenuEntry {
        path: r"SOFTWARE\Classes\Directory\shell\mcopy_paste",
        label: "Paste here with mcopy",
        command_template: r#""{exe}" paste "%1""#,
        multi_select: false,
    },
    // "Paste with mcopy" for drive roots such as D:\ or E:\.
    MenuEntry {
        path: r"SOFTWARE\Classes\Drive\shell\mcopy_paste",
        label: "Paste with mcopy",
        command_template: r#""{exe}" paste "%1""#,
        multi_select: false,
    },
];

/// The entry whose version we trust as the authoritative install marker.
const PRIMARY_MENU_PATH: &str =
    r"SOFTWARE\Classes\Directory\Background\shell\mcopy_paste";

pub struct WindowsMenu;

impl ContextMenu for WindowsMenu {
    fn install(exe_path: &Path) -> anyhow::Result<()> {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let exe_str = exe_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("The executable path is invalid"))?;

        for entry in MENU_ENTRIES {
            install_entry(&hklm, exe_str, entry)?;
        }

        println!("✓ Context menu installed successfully!");
        Ok(())
    }

    fn uninstall() -> anyhow::Result<()> {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

        for entry in MENU_ENTRIES {
            delete_entry(&hklm, entry)?;
        }

        println!("✓ Context menu removed successfully!");
        Ok(())
    }

    fn state() -> anyhow::Result<ContextMenuInstallState> {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

        if let Ok(key) =
            hklm.open_subkey_with_flags(PRIMARY_MENU_PATH, KEY_READ)
        {
            return Ok(ContextMenuInstallState::Installed {
                version: read_version(&key),
            });
        }

        for entry in MENU_ENTRIES {
            if hklm.open_subkey_with_flags(entry.path, KEY_READ).is_ok() {
                return Ok(ContextMenuInstallState::Installed {
                    version: None,
                });
            }
        }

        Ok(ContextMenuInstallState::NotInstalled)
    }
}

/// Create one menu entry plus its `command` subkey.
fn install_entry(
    hklm: &RegKey,
    exe_path: &str,
    entry: &MenuEntry,
) -> anyhow::Result<()> {
    let (key, _) = hklm.create_subkey(entry.path)?;
    key.set_value("", &entry.label)?;
    write_metadata(&key, exe_path)?;

    // Explorer invokes the command once per selected item; `--append`
    // lets every invocation extend the shared clipboard session.
    if entry.multi_select {
        key.set_value("MultiSelectModel", &"Player")?;
    }

    let (cmd_key, _) =
        hklm.create_subkey(format!("{}\\command", entry.path))?;
    let command = entry.command_template.replace("{exe}", exe_path);
    cmd_key.set_value("", &command)?;

    Ok(())
}

/// Delete one menu entry, splitting its path into base key + entry name.
fn delete_entry(hklm: &RegKey, entry: &MenuEntry) -> anyhow::Result<()> {
    let (base_path, menu_name) = entry
        .path
        .rsplit_once('\\')
        .ok_or_else(|| anyhow::anyhow!("Invalid menu path: {}", entry.path))?;
    delete_menu_entry(hklm, base_path, menu_name)
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
    hklm: &RegKey,
    base_path: &str,
    menu_name: &str,
) -> anyhow::Result<()> {
    match hklm.open_subkey_with_flags(base_path, KEY_WRITE) {
        Ok(key) => match key.delete_subkey_all(menu_name) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(anyhow::anyhow!("Registry delete error: {}", e)),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(anyhow::anyhow!("Registry open error: {}", e)),
    }
}
