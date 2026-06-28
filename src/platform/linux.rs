use super::{ContextMenu, ContextMenuInstallState};
use crate::platform::state::CURRENT_VERSION;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const SUPPORT_DIR: &str = ".local/share/mcopy";
const VERSION_FILE: &str = "install-version";

pub struct LinuxMenu;

impl ContextMenu for LinuxMenu {
    /// Install Linux file manager integration (Nautilus, Dolphin, Thunar).
    fn install(exe_path: &Path) -> anyhow::Result<()> {
        let home = std::env::var("HOME")?;
        let exe_str = exe_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("The executable path is invalid"))?;

        // Nautilus scripts.
        install_nautilus_scripts(&home, exe_str)?;

        // Dolphin (KDE) service menus.
        install_dolphin_service(&home, exe_str)?;

        // Thunar (XFCE) custom actions.
        install_thunar_actions(exe_str)?;

        write_install_metadata(&home)?;

        println!("✓ File manager integration installed successfully!");
        Ok(())
    }

    /// Remove Linux file manager integration.
    fn uninstall() -> anyhow::Result<()> {
        let home = std::env::var("HOME")?;

        // Nautilus.
        let nautilus_dir = PathBuf::from(&home).join(".local/share/nautilus/scripts");
        let _ = fs::remove_file(nautilus_dir.join("mcopy-copy"));
        let _ = fs::remove_file(nautilus_dir.join("mcopy-paste"));

        // Dolphin.
        let dolphin_dir = PathBuf::from(&home).join(".local/share/kservices5/ServiceMenus");
        let _ = fs::remove_file(dolphin_dir.join("mcopy.desktop"));

        // Thunar uses `uca.xml`, which is more complicated to edit safely.
        println!("✓ Nautilus and Dolphin integration removed!");
        println!("  Note: Remove the Thunar actions manually from Edit > Configure custom actions");

        remove_install_metadata(&home)?;

        Ok(())
    }

    fn state() -> anyhow::Result<ContextMenuInstallState> {
        let home = std::env::var("HOME")?;
        let version_path = version_file_path(&home);

        if let Ok(version) = fs::read_to_string(&version_path) {
            let version = version.trim().to_string();
            if !version.is_empty() {
                return Ok(ContextMenuInstallState::Installed {
                    version: Some(version),
                });
            }
        }

        let nautilus_dir = PathBuf::from(&home).join(".local/share/nautilus/scripts");
        let dolphin_dir = PathBuf::from(&home).join(".local/share/kservices5/ServiceMenus");

        if nautilus_dir.join("mcopy-copy").exists()
            || nautilus_dir.join("mcopy-paste").exists()
            || dolphin_dir.join("mcopy.desktop").exists()
        {
            return Ok(ContextMenuInstallState::Installed { version: None });
        }

        Ok(ContextMenuInstallState::NotInstalled)
    }
}

fn write_install_metadata(home: &str) -> anyhow::Result<()> {
    let support_dir = PathBuf::from(home).join(SUPPORT_DIR);
    fs::create_dir_all(&support_dir)?;
    fs::write(support_dir.join(VERSION_FILE), CURRENT_VERSION)?;
    Ok(())
}

fn remove_install_metadata(home: &str) -> anyhow::Result<()> {
    let _ = fs::remove_file(version_file_path(home));
    Ok(())
}

fn version_file_path(home: &str) -> PathBuf {
    PathBuf::from(home).join(SUPPORT_DIR).join(VERSION_FILE)
}

fn install_nautilus_scripts(home: &str, exe_path: &str) -> anyhow::Result<()> {
    let scripts_dir = PathBuf::from(home).join(".local/share/nautilus/scripts");
    fs::create_dir_all(&scripts_dir)?;

    // Copy script.
    let copy_script = format!(
        r#"#!/bin/bash
# mcopy - Copy selected files/folders
for arg in "$@"; do
    "{}" copy "$arg"
done
"#,
        exe_path
    );
    let copy_path = scripts_dir.join("mcopy-copy");
    fs::write(&copy_path, copy_script)?;
    fs::set_permissions(&copy_path, fs::Permissions::from_mode(0o755))?;

    // Paste script.
    let paste_script = format!(
        r#"#!/bin/bash
# mcopy - Paste to current directory
"{}" paste "$NAUTILUS_SCRIPT_CURRENT_URI"
"#,
        exe_path
    );
    let paste_path = scripts_dir.join("mcopy-paste");
    fs::write(&paste_path, paste_script)?;
    fs::set_permissions(&paste_path, fs::Permissions::from_mode(0o755))?;

    println!("  Nautilus: {}", scripts_dir.display());
    Ok(())
}

fn install_dolphin_service(home: &str, exe_path: &str) -> anyhow::Result<()> {
    let services_dir = PathBuf::from(home).join(".local/share/kservices5/ServiceMenus");
    fs::create_dir_all(&services_dir)?;

    let desktop_entry = format!(
        r#"[Desktop Entry]
Type=Service
ServiceTypes=KonqPopupMenu/Plugin
MimeType=all/all;
Actions=mcopy_copy;mcopy_paste;

[Desktop Action mcopy_copy]
Name=Copy with mcopy
Icon=edit-copy
Exec="{}" copy %f

[Desktop Action mcopy_paste]
Name=Paste with mcopy
Icon=edit-paste
Exec="{}" paste %d
"#,
        exe_path, exe_path
    );
    fs::write(services_dir.join("mcopy.desktop"), desktop_entry)?;

    println!("  Dolphin: {}", services_dir.display());
    Ok(())
}

fn install_thunar_actions(exe_path: &str) -> anyhow::Result<()> {
    // Thunar uses `uca.xml`, which is harder to edit programmatically.
    // Print setup guidance instead.
    println!("  Thunar: manual setup required");
    println!("    1. Edit > Configure custom actions");
    println!(
        "    2. Add: Name='mcopy Copy', Command='{} copy %f'",
        exe_path
    );
    println!(
        "    3. Add: Name='mcopy Paste', Command='{} paste %d'",
        exe_path
    );
    Ok(())
}
