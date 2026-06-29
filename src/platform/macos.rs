use super::{ContextMenu, ContextMenuInstallState};
use crate::platform::state::CURRENT_VERSION;
use std::fs;
use std::path::{Path, PathBuf};

const SERVICES_DIR: &str = "Library/Services";
const SUPPORT_DIR: &str = "Library/Application Support/mcopy";
const VERSION_FILE: &str = "install-version";
const COPY_SERVICE_NAME: &str = "Copy with mcopy";
const PASTE_SERVICE_NAME: &str = "Paste with mcopy";
const LEGACY_SERVICE_NAMES: &[&str] = &["mcopy Copy", "mcopy Paste"];

pub struct MacosMenu;

impl ContextMenu for MacosMenu {
    /// Install macOS Finder Services.
    fn install(exe_path: &Path) -> anyhow::Result<()> {
        let home = home_dir()?;
        let services_dir = PathBuf::from(&home).join(SERVICES_DIR);

        // Create the Services directory.
        fs::create_dir_all(&services_dir)?;

        let exe_str = exe_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("The executable path is invalid"))?;

        // mcopy Copy workflow
        create_automator_workflow(
            &services_dir,
            COPY_SERVICE_NAME,
            exe_str,
            "copy",
        )?;

        // mcopy Paste workflow
        create_automator_workflow(
            &services_dir,
            PASTE_SERVICE_NAME,
            exe_str,
            "paste",
        )?;

        write_install_metadata(&home)?;

        // Nudge the pasteboard server to re-scan ~/Library/Services so the new
        // workflows show up without a re-login (best-effort).
        let _ = std::process::Command::new("/System/Library/CoreServices/pbs")
            .arg("-update")
            .status();

        println!("✓ Finder Services installed successfully!");
        println!("  Location: {}", services_dir.display());
        println!(
            "  Note: Enable them in System Preferences > Keyboard > Shortcuts > Services"
        );
        Ok(())
    }

    /// Remove macOS Finder Services.
    fn uninstall() -> anyhow::Result<()> {
        let home = home_dir()?;
        let services_dir = PathBuf::from(&home).join(SERVICES_DIR);

        // Remove current and legacy workflow bundles so renames do not leave
        // duplicate Finder Services behind.
        for name in current_and_legacy_service_names() {
            let workflow = workflow_path(&services_dir, name);
            if workflow.exists() {
                fs::remove_dir_all(workflow)?;
            }
        }

        remove_install_metadata(&home)?;

        println!("✓ Finder Services removed successfully!");
        Ok(())
    }

    fn state() -> anyhow::Result<ContextMenuInstallState> {
        let home = home_dir()?;
        let version_path = version_file_path(&home);

        let services_dir = PathBuf::from(&home).join(SERVICES_DIR);

        if let Ok(version) = fs::read_to_string(&version_path) {
            let version = version.trim().to_string();
            if !version.is_empty() {
                if version == CURRENT_VERSION
                    && !workflows_are_current(&services_dir)
                {
                    return Ok(ContextMenuInstallState::Installed {
                        version: None,
                    });
                }

                return Ok(ContextMenuInstallState::Installed {
                    version: Some(version),
                });
            }
        }

        let copy_workflow = workflow_path(&services_dir, COPY_SERVICE_NAME);
        let paste_workflow = workflow_path(&services_dir, PASTE_SERVICE_NAME);

        if copy_workflow.exists() || paste_workflow.exists() {
            return Ok(ContextMenuInstallState::Installed { version: None });
        }

        Ok(ContextMenuInstallState::NotInstalled)
    }
}

/// Resolve the user's home directory via `dirs` (more robust than `$HOME` and
/// consistent with how Windows resolves paths).
fn home_dir() -> anyhow::Result<String> {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| {
            anyhow::anyhow!("Could not determine the home directory")
        })
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

fn workflow_path(services_dir: &Path, name: &str) -> PathBuf {
    services_dir.join(format!("{}.workflow", name))
}

fn current_and_legacy_service_names() -> impl Iterator<Item = &'static str> {
    [COPY_SERVICE_NAME, PASTE_SERVICE_NAME]
        .into_iter()
        .chain(LEGACY_SERVICE_NAMES.iter().copied())
}

fn workflows_are_current(services_dir: &Path) -> bool {
    workflow_is_current(services_dir, COPY_SERVICE_NAME)
        && workflow_is_current(services_dir, PASTE_SERVICE_NAME)
}

fn workflow_is_current(services_dir: &Path, name: &str) -> bool {
    let workflow_dir = workflow_path(services_dir, name);
    workflow_dir.join("Contents/Info.plist").is_file()
        && workflow_dir
            .join("Contents/Resources/document.wflow")
            .is_file()
}

fn create_automator_workflow(
    services_dir: &Path,
    name: &str,
    exe_path: &str,
    action: &str,
) -> anyhow::Result<()> {
    let workflow_dir = workflow_path(services_dir, name);
    let contents_dir = workflow_dir.join("Contents");
    let resources_dir = contents_dir.join("Resources");

    fs::create_dir_all(&resources_dir)?;

    let escaped_name = xml_escape(name);
    let bundle_identifier = format!("com.mcopy.service.{}", action);

    // Info.plist
    let info_plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleIdentifier</key>
    <string>{}</string>
    <key>CFBundleName</key>
    <string>{}</string>
    <key>CFBundleShortVersionString</key>
    <string>{}</string>
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
            <key>NSRequiredContext</key>
            <dict>
                <key>NSApplicationIdentifier</key>
                <string>com.apple.finder</string>
            </dict>
            <key>NSSendFileTypes</key>
            <array>
                <string>public.item</string>
            </array>
        </dict>
    </array>
</dict>
</plist>"#,
        bundle_identifier, escaped_name, CURRENT_VERSION, escaped_name
    );
    fs::write(contents_dir.join("Info.plist"), info_plist)?;

    let command_string = workflow_command(exe_path, action)?;
    let command_string = xml_escape(&command_string);

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
                    <string>{}</string>
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
        <key>serviceApplicationBundleID</key>
        <string>com.apple.finder</string>
        <key>serviceApplicationPath</key>
        <string>/System/Library/CoreServices/Finder.app</string>
        <key>serviceInputTypeIdentifier</key>
        <string>com.apple.Automator.fileSystemObject</string>
        <key>serviceOutputTypeIdentifier</key>
        <string>com.apple.Automator.nothing</string>
        <key>serviceProcessesInput</key>
        <integer>1</integer>
        <key>workflowTypeIdentifier</key>
        <string>com.apple.Automator.servicesMenu</string>
    </dict>
</dict>
</plist>"#,
        command_string
    );
    fs::write(resources_dir.join("document.wflow"), wflow)?;
    let _ = fs::remove_file(contents_dir.join("document.wflow"));

    Ok(())
}

fn workflow_command(exe_path: &str, action: &str) -> anyhow::Result<String> {
    let exe = shell_quote(exe_path);

    match action {
        "copy" => Ok(format!("{exe} copy \"$@\"")),
        "paste" => Ok(format!("for f in \"$@\"; do {exe} paste \"$f\"; done")),
        _ => anyhow::bail!("Unsupported macOS workflow action: {action}"),
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
