use std::path::PathBuf;

// ============================================================================
// Windows Implementation
// ============================================================================
#[cfg(target_os = "windows")]
mod windows_impl {
    use super::*;
    use winreg::RegKey;
    use winreg::enums::*;

    /// Context menu'yu registry'ye kur
    pub fn install_context_menu(exe_path: &PathBuf) -> anyhow::Result<()> {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let exe_str = exe_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Exe path geçersiz"))?;

        // Dosyalar için "mcopy ile kopyala"
        install_for_files(&hklm, exe_str)?;

        // Klasörler için "mcopy ile kopyala"
        install_for_directories(&hklm, exe_str)?;

        // Boş alan için "mcopy ile yapıştır"
        install_paste_background(&hklm, exe_str)?;

        // Klasör üzerine sağ tık için "mcopy ile yapıştır"
        install_paste_directory(&hklm, exe_str)?;

        // Disk kökü için "mcopy ile yapıştır" (D:\, E:\ vb.)
        install_paste_drive(&hklm, exe_str)?;

        println!("✓ Context menu başarıyla yüklendi!");
        Ok(())
    }

    /// Context menu'yu registry'den kaldır
    pub fn uninstall_context_menu() -> anyhow::Result<()> {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

        // Tüm entry'leri sil
        delete_menu_entry(&hklm, r"SOFTWARE\Classes\*\shell", "mcopy_copy")?;
        delete_menu_entry(&hklm, r"SOFTWARE\Classes\Directory\shell", "mcopy_copy")?;
        delete_menu_entry(&hklm, r"SOFTWARE\Classes\Directory\shell", "mcopy_paste")?;
        delete_menu_entry(
            &hklm,
            r"SOFTWARE\Classes\Directory\Background\shell",
            "mcopy_paste",
        )?;
        delete_menu_entry(&hklm, r"SOFTWARE\Classes\Drive\shell", "mcopy_paste")?;

        println!("✓ Context menu başarıyla kaldırıldı!");
        Ok(())
    }

    /// Dosyalar için context menu kur (çoklu seçim destekli)
    fn install_for_files(hklm: &RegKey, exe_path: &str) -> anyhow::Result<()> {
        let path = r"SOFTWARE\Classes\*\shell\mcopy_copy";
        let (key, _) = hklm.create_subkey(path)?;
        key.set_value("", &"mcopy ile kopyala")?;
        // Çoklu seçimde her dosya için ayrı çağrı yapılır, --append ile clipboard'a eklenir
        key.set_value("MultiSelectModel", &"Player")?;

        let (cmd_key, _) = hklm.create_subkey(format!("{}\\command", path))?;
        // --append kullanarak çoklu seçimde tüm dosyalar clipboard'a eklenir
        cmd_key.set_value("", &format!("\"{}\" copy --append \"%1\"", exe_path))?;

        Ok(())
    }

    /// Klasörler için context menu kur (çoklu seçim destekli)
    fn install_for_directories(hklm: &RegKey, exe_path: &str) -> anyhow::Result<()> {
        let path = r"SOFTWARE\Classes\Directory\shell\mcopy_copy";
        let (key, _) = hklm.create_subkey(path)?;
        key.set_value("", &"mcopy ile kopyala")?;
        // Çoklu seçimde her klasör için ayrı çağrı yapılır
        key.set_value("MultiSelectModel", &"Player")?;

        let (cmd_key, _) = hklm.create_subkey(format!("{}\\command", path))?;
        // --append kullanarak çoklu seçimde tüm klasörler clipboard'a eklenir
        cmd_key.set_value("", &format!("\"{}\" copy --append \"%1\"", exe_path))?;

        Ok(())
    }

    /// Boş alan (background) için "mcopy ile yapıştır"
    fn install_paste_background(hklm: &RegKey, exe_path: &str) -> anyhow::Result<()> {
        let path = r"SOFTWARE\Classes\Directory\Background\shell\mcopy_paste";
        let (key, _) = hklm.create_subkey(path)?;
        key.set_value("", &"mcopy ile yapıştır")?;

        let (cmd_key, _) = hklm.create_subkey(format!("{}\\command", path))?;
        cmd_key.set_value("", &format!("\"{}\" paste \"%V\"", exe_path))?;

        Ok(())
    }

    /// Klasör üzerine sağ tık için "mcopy ile yapıştır"
    fn install_paste_directory(hklm: &RegKey, exe_path: &str) -> anyhow::Result<()> {
        let path = r"SOFTWARE\Classes\Directory\shell\mcopy_paste";
        let (key, _) = hklm.create_subkey(path)?;
        key.set_value("", &"mcopy ile buraya yapıştır")?;

        let (cmd_key, _) = hklm.create_subkey(format!("{}\\command", path))?;
        cmd_key.set_value("", &format!("\"{}\" paste \"%1\"", exe_path))?;

        Ok(())
    }

    /// Disk kökü için "mcopy ile yapıştır" (D:\, E:\ vb.)
    fn install_paste_drive(hklm: &RegKey, exe_path: &str) -> anyhow::Result<()> {
        let path = r"SOFTWARE\Classes\Drive\shell\mcopy_paste";
        let (key, _) = hklm.create_subkey(path)?;
        key.set_value("", &"mcopy ile yapıştır")?;

        let (cmd_key, _) = hklm.create_subkey(format!("{}\\command", path))?;
        cmd_key.set_value("", &format!("\"{}\" paste \"%1\"", exe_path))?;

        Ok(())
    }

    /// Menu entry'yi sil
    fn delete_menu_entry(hklm: &RegKey, base_path: &str, menu_name: &str) -> anyhow::Result<()> {
        match hklm.open_subkey_with_flags(base_path, KEY_WRITE) {
            Ok(key) => match key.delete_subkey_all(menu_name) {
                Ok(_) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(anyhow::anyhow!("Registry silme hatası: {}", e)),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(anyhow::anyhow!("Registry açma hatası: {}", e)),
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
    use std::os::unix::fs::PermissionsExt;

    const SERVICES_DIR: &str = "Library/Services";

    /// macOS Finder Services kurulumu
    pub fn install_context_menu(exe_path: &PathBuf) -> anyhow::Result<()> {
        let home = std::env::var("HOME")?;
        let services_dir = PathBuf::from(&home).join(SERVICES_DIR);

        // Services dizinini oluştur
        fs::create_dir_all(&services_dir)?;

        let exe_str = exe_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Exe path geçersiz"))?;

        // mcopy Copy workflow
        create_automator_workflow(&services_dir, "mcopy Copy", exe_str, "copy")?;

        // mcopy Paste workflow
        create_automator_workflow(&services_dir, "mcopy Paste", exe_str, "paste")?;

        println!("✓ Finder Services başarıyla yüklendi!");
        println!("  Konum: {}", services_dir.display());
        println!("  Not: System Preferences > Keyboard > Shortcuts > Services'dan etkinleştirin");
        Ok(())
    }

    /// macOS Finder Services kaldırma
    pub fn uninstall_context_menu() -> anyhow::Result<()> {
        let home = std::env::var("HOME")?;
        let services_dir = PathBuf::from(&home).join(SERVICES_DIR);

        // Workflow'ları sil
        let copy_workflow = services_dir.join("mcopy Copy.workflow");
        let paste_workflow = services_dir.join("mcopy Paste.workflow");

        if copy_workflow.exists() {
            fs::remove_dir_all(&copy_workflow)?;
        }
        if paste_workflow.exists() {
            fs::remove_dir_all(&paste_workflow)?;
        }

        println!("✓ Finder Services başarıyla kaldırıldı!");
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

    /// Linux file manager entegrasyonu (Nautilus, Dolphin, Thunar)
    pub fn install_context_menu(exe_path: &PathBuf) -> anyhow::Result<()> {
        let home = std::env::var("HOME")?;
        let exe_str = exe_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Exe path geçersiz"))?;

        // Nautilus scripts
        install_nautilus_scripts(&home, exe_str)?;

        // Dolphin (KDE) service menus
        install_dolphin_service(&home, exe_str)?;

        // Thunar (XFCE) custom actions
        install_thunar_actions(&home, exe_str)?;

        println!("✓ File manager entegrasyonu başarıyla yüklendi!");
        Ok(())
    }

    /// Linux file manager entegrasyonunu kaldır
    pub fn uninstall_context_menu() -> anyhow::Result<()> {
        let home = std::env::var("HOME")?;

        // Nautilus
        let nautilus_dir = PathBuf::from(&home).join(".local/share/nautilus/scripts");
        let _ = fs::remove_file(nautilus_dir.join("mcopy-copy"));
        let _ = fs::remove_file(nautilus_dir.join("mcopy-paste"));

        // Dolphin
        let dolphin_dir = PathBuf::from(&home).join(".local/share/kservices5/ServiceMenus");
        let _ = fs::remove_file(dolphin_dir.join("mcopy.desktop"));

        // Thunar - uca.xml düzenlenmeli ama karmaşık, kullanıcıya bilgi ver
        println!("✓ Nautilus ve Dolphin entegrasyonu kaldırıldı!");
        println!("  Not: Thunar için manuel olarak Edit > Configure custom actions'dan kaldırın");

        Ok(())
    }

    fn install_nautilus_scripts(home: &str, exe_path: &str) -> anyhow::Result<()> {
        let scripts_dir = PathBuf::from(home).join(".local/share/nautilus/scripts");
        fs::create_dir_all(&scripts_dir)?;

        // Copy script
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

        // Paste script
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
Name=mcopy ile kopyala
Icon=edit-copy
Exec="{}" copy %f

[Desktop Action mcopy_paste]
Name=mcopy ile yapıştır
Icon=edit-paste
Exec="{}" paste %d
"#,
            exe_path, exe_path
        );
        fs::write(services_dir.join("mcopy.desktop"), desktop_entry)?;

        println!("  Dolphin: {}", services_dir.display());
        Ok(())
    }

    fn install_thunar_actions(home: &str, exe_path: &str) -> anyhow::Result<()> {
        // Thunar uses uca.xml which is more complex to edit programmatically
        // Provide instructions instead
        println!("  Thunar: Manuel kurulum gerekli");
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
pub fn install_context_menu(_exe_path: &PathBuf) -> anyhow::Result<()> {
    anyhow::bail!("Bu platform için context menu entegrasyonu desteklenmiyor")
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn uninstall_context_menu() -> anyhow::Result<()> {
    anyhow::bail!("Bu platform için context menu entegrasyonu desteklenmiyor")
}
