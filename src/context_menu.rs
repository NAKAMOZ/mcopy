use std::path::Path;

// ============================================================================
// Windows Implementation
// ============================================================================
#[cfg(target_os = "windows")]
mod windows_impl {
    use super::*;
    use winreg::RegKey;
    use winreg::enums::*;

    /// Install the context menu into the registry.
    pub fn install_context_menu(exe_path: &Path) -> anyhow::Result<()> {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let exe_str = exe_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("The executable path is invalid"))?;

        // "Copy with mcopy" for files.
        install_for_files(&hklm, exe_str)?;

        // "Copy with mcopy" for directories.
        install_for_directories(&hklm, exe_str)?;

        // "Paste with mcopy" for the directory background.
        install_paste_background(&hklm, exe_str)?;

        // "Paste here with mcopy" for a directory entry.
        install_paste_directory(&hklm, exe_str)?;

        // "Paste with mcopy" for drive roots such as D:\ or E:\.
        install_paste_drive(&hklm, exe_str)?;

        println!("✓ Context menu installed successfully!");
        Ok(())
    }

    /// Remove the context menu from the registry.
    pub fn uninstall_context_menu() -> anyhow::Result<()> {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

        // Delete every registered entry.
        delete_menu_entry(&hklm, r"SOFTWARE\Classes\*\shell", "mcopy_copy")?;
        delete_menu_entry(&hklm, r"SOFTWARE\Classes\Directory\shell", "mcopy_copy")?;
        delete_menu_entry(&hklm, r"SOFTWARE\Classes\Directory\shell", "mcopy_paste")?;
        delete_menu_entry(
            &hklm,
            r"SOFTWARE\Classes\Directory\Background\shell",
            "mcopy_paste",
        )?;
        delete_menu_entry(&hklm, r"SOFTWARE\Classes\Drive\shell", "mcopy_paste")?;

        println!("✓ Context menu removed successfully!");
        Ok(())
    }

    /// Install the file context menu entry with multi-select support.
    fn install_for_files(hklm: &RegKey, exe_path: &str) -> anyhow::Result<()> {
        let path = r"SOFTWARE\Classes\*\shell\mcopy_copy";
        let (key, _) = hklm.create_subkey(path)?;
        key.set_value("", &"Copy with mcopy")?;
        // Explorer invokes the command once per selected item; `--append`
        // lets every invocation extend the shared clipboard session.
        key.set_value("MultiSelectModel", &"Player")?;

        let (cmd_key, _) = hklm.create_subkey(format!("{}\\command", path))?;
        // Append every selected file into the clipboard payload.
        cmd_key.set_value("", &format!("\"{}\" copy --append \"%1\"", exe_path))?;

        Ok(())
    }

    /// Install the directory context menu entry with multi-select support.
    fn install_for_directories(hklm: &RegKey, exe_path: &str) -> anyhow::Result<()> {
        let path = r"SOFTWARE\Classes\Directory\shell\mcopy_copy";
        let (key, _) = hklm.create_subkey(path)?;
        key.set_value("", &"Copy with mcopy")?;
        // Explorer invokes the command once per selected folder.
        key.set_value("MultiSelectModel", &"Player")?;

        let (cmd_key, _) = hklm.create_subkey(format!("{}\\command", path))?;
        // Append every selected directory into the clipboard payload.
        cmd_key.set_value("", &format!("\"{}\" copy --append \"%1\"", exe_path))?;

        Ok(())
    }

    /// Install "Paste with mcopy" on the directory background.
    fn install_paste_background(hklm: &RegKey, exe_path: &str) -> anyhow::Result<()> {
        let path = r"SOFTWARE\Classes\Directory\Background\shell\mcopy_paste";
        let (key, _) = hklm.create_subkey(path)?;
        key.set_value("", &"Paste with mcopy")?;

        let (cmd_key, _) = hklm.create_subkey(format!("{}\\command", path))?;
        cmd_key.set_value("", &format!("\"{}\" paste \"%V\"", exe_path))?;

        Ok(())
    }

    /// Install "Paste here with mcopy" on a directory entry.
    fn install_paste_directory(hklm: &RegKey, exe_path: &str) -> anyhow::Result<()> {
        let path = r"SOFTWARE\Classes\Directory\shell\mcopy_paste";
        let (key, _) = hklm.create_subkey(path)?;
        key.set_value("", &"Paste here with mcopy")?;

        let (cmd_key, _) = hklm.create_subkey(format!("{}\\command", path))?;
        cmd_key.set_value("", &format!("\"{}\" paste \"%1\"", exe_path))?;

        Ok(())
    }

    /// Install "Paste with mcopy" for drive roots such as D:\ or E:\.
    fn install_paste_drive(hklm: &RegKey, exe_path: &str) -> anyhow::Result<()> {
        let path = r"SOFTWARE\Classes\Drive\shell\mcopy_paste";
        let (key, _) = hklm.create_subkey(path)?;
        key.set_value("", &"Paste with mcopy")?;

        let (cmd_key, _) = hklm.create_subkey(format!("{}\\command", path))?;
        cmd_key.set_value("", &format!("\"{}\" paste \"%1\"", exe_path))?;

        Ok(())
    }

    /// Delete a single menu entry if it exists.
    fn delete_menu_entry(hklm: &RegKey, base_path: &str, menu_name: &str) -> anyhow::Result<()> {
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
}

// ============================================================================
// macOS Implementation
// ============================================================================
#[cfg(target_os = "macos")]
mod macos_impl {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    const SERVICES_DIR: &str = "Library/Services";

    /// Install macOS Finder Services.
    pub fn install_context_menu(exe_path: &Path) -> anyhow::Result<()> {
        let home = std::env::var("HOME")?;
        let services_dir = PathBuf::from(&home).join(SERVICES_DIR);

        // Create the Services directory.
        fs::create_dir_all(&services_dir)?;

        let exe_str = exe_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("The executable path is invalid"))?;

        // mcopy Copy workflow
        create_automator_workflow(&services_dir, "mcopy Copy", exe_str, "copy")?;

        // mcopy Paste workflow
        create_automator_workflow(&services_dir, "mcopy Paste", exe_str, "paste")?;

        println!("✓ Finder Services installed successfully!");
        println!("  Location: {}", services_dir.display());
        println!("  Note: Enable them in System Preferences > Keyboard > Shortcuts > Services");
        Ok(())
    }

    /// Remove macOS Finder Services.
    pub fn uninstall_context_menu() -> anyhow::Result<()> {
        let home = std::env::var("HOME")?;
        let services_dir = PathBuf::from(&home).join(SERVICES_DIR);

        // Remove the workflow bundles.
        let copy_workflow = services_dir.join("mcopy Copy.workflow");
        let paste_workflow = services_dir.join("mcopy Paste.workflow");

        if copy_workflow.exists() {
            fs::remove_dir_all(&copy_workflow)?;
        }
        if paste_workflow.exists() {
            fs::remove_dir_all(&paste_workflow)?;
        }

        println!("✓ Finder Services removed successfully!");
        Ok(())
    }

    fn create_automator_workflow(
        services_dir: &PathBuf,
        name: &str,
        exe_path: &str,
        action: &str,
    ) -> anyhow::Result<()> {
        let workflow_dir = services_dir.join(format!("{}.workflow", name));
        let contents_dir = workflow_dir.join("Contents");

        fs::create_dir_all(&contents_dir)?;

        // Info.plist
        let info_plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>NSServices</key>
    <array>
        <dict>
            <key>NSMenuItem</key>
            <dict>
                <key>default</key>
                <string>{}</string>
            </dict>
            <key>NSMessage</key>
            <string>runWorkflowAsService</string>
            <key>NSSendFileTypes</key>
            <array>
                <string>public.item</string>
            </array>
        </dict>
    </array>
</dict>
</plist>"#,
            name
        );
        fs::write(contents_dir.join("Info.plist"), info_plist)?;

        // document.wflow - Shell script action
        let wflow = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>AMApplicationBuild</key>
    <string>523</string>
    <key>AMApplicationVersion</key>
    <string>2.10</string>
    <key>AMDocumentVersion</key>
    <string>2</string>
    <key>actions</key>
    <array>
        <dict>
            <key>action</key>
            <dict>
                <key>AMAccepts</key>
                <dict>
                    <key>Container</key>
                    <string>List</string>
                    <key>Optional</key>
                    <true/>
                    <key>Types</key>
                    <array>
                        <string>com.apple.cocoa.path</string>
                    </array>
                </dict>
                <key>AMActionVersion</key>
                <string>1.0.2</string>
                <key>AMApplication</key>
                <array>
                    <string>Automator</string>
                </array>
                <key>AMCategory</key>
                <string>AMCategoryUtilities</string>
                <key>AMIconName</key>
                <string>RunShellScript</string>
                <key>AMName</key>
                <string>Run Shell Script</string>
                <key>AMProvides</key>
                <dict>
                    <key>Container</key>
                    <string>List</string>
                    <key>Types</key>
                    <array>
                        <string>com.apple.cocoa.string</string>
                    </array>
                </dict>
                <key>ActionBundlePath</key>
                <string>/System/Library/Automator/Run Shell Script.action</string>
                <key>ActionName</key>
                <string>Run Shell Script</string>
                <key>ActionParameters</key>
                <dict>
                    <key>COMMAND_STRING</key>
                    <string>for f in "$@"; do "{}" {} "$f"; done</string>
                    <key>CheckedForUserDefaultShell</key>
                    <true/>
                    <key>inputMethod</key>
                    <integer>1</integer>
                    <key>shell</key>
                    <string>/bin/zsh</string>
                    <key>source</key>
                    <string></string>
                </dict>
                <key>BundleIdentifier</key>
                <string>com.apple.RunShellScript</string>
                <key>CFBundleVersion</key>
                <string>1.0.2</string>
                <key>CanShowSelectedItemsWhenRun</key>
                <false/>
                <key>CanShowWhenRun</key>
                <true/>
                <key>Category</key>
                <array>
                    <string>AMCategoryUtilities</string>
                </array>
                <key>Class Name</key>
                <string>RunShellScriptAction</string>
                <key>InputUUID</key>
                <string>0</string>
                <key>Keywords</key>
                <array>
                    <string>Shell</string>
                    <string>Script</string>
                    <string>Command</string>
                    <string>Run</string>
                    <string>Unix</string>
                </array>
                <key>OutputUUID</key>
                <string>0</string>
                <key>UUID</key>
                <string>0</string>
                <key>UnlocalizedApplications</key>
                <array>
                    <string>Automator</string>
                </array>
                <key>arguments</key>
                <dict/>
                <key>conversionLabel</key>
                <integer>0</integer>
                <key>isViewVisible</key>
                <integer>1</integer>
                <key>location</key>
                <string>309.000000:253.000000</string>
                <key>nibPath</key>
                <string>/System/Library/Automator/Run Shell Script.action/Contents/Resources/Base.lproj/main.nib</string>
            </dict>
            <key>isViewVisible</key>
            <integer>1</integer>
        </dict>
    </array>
    <key>connectors</key>
    <dict/>
    <key>workflowMetaData</key>
    <dict>
        <key>workflowTypeIdentifier</key>
        <string>com.apple.Automator.servicesMenu</string>
    </dict>
</dict>
</plist>"#,
            exe_path, action
        );
        fs::write(contents_dir.join("document.wflow"), wflow)?;

        Ok(())
    }
}

// ============================================================================
// Linux Implementation
// ============================================================================
#[cfg(target_os = "linux")]
mod linux_impl {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    /// Install Linux file manager integration (Nautilus, Dolphin, Thunar).
    pub fn install_context_menu(exe_path: &Path) -> anyhow::Result<()> {
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

        println!("✓ File manager integration installed successfully!");
        Ok(())
    }

    /// Remove Linux file manager integration.
    pub fn uninstall_context_menu() -> anyhow::Result<()> {
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

        Ok(())
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
}

// ============================================================================
// Public API - Platform-agnostic exports
// ============================================================================
#[cfg(target_os = "windows")]
pub use windows_impl::{install_context_menu, uninstall_context_menu};

#[cfg(target_os = "macos")]
pub use macos_impl::{install_context_menu, uninstall_context_menu};

#[cfg(target_os = "linux")]
pub use linux_impl::{install_context_menu, uninstall_context_menu};

// Fallback for unsupported platforms
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn install_context_menu(_exe_path: &Path) -> anyhow::Result<()> {
    anyhow::bail!("Context menu integration is not supported on this platform")
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn uninstall_context_menu() -> anyhow::Result<()> {
    anyhow::bail!("Context menu integration is not supported on this platform")
}
