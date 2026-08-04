//! Platform shell-integration behavior, exercised against the real OS.
//!
//! These tests write to the *actual* per-user integration points, because that
//! is the thing under test: nothing here needs elevation in 0.3, which is
//! precisely the property being asserted.
//!
//! They run serially and restore the prior state, so a developer's own mcopy
//! installation survives a test run. Anything that would require administrator
//! or root rights is skipped rather than attempted.

use mcopy::platform::{
    self, ContextMenu, ContextMenuInstallState, PasteVisibility, Platform,
};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

/// The tests mutate one shared resource (the user's shell integration), so they
/// must not overlap. `cargo test` runs integration tests in one binary on
/// several threads by default.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Restores whatever integration state existed before the test.
struct Restore {
    was_installed: bool,
}

impl Restore {
    fn capture() -> Self {
        let was_installed = matches!(
            Platform::state(),
            Ok(ContextMenuInstallState::Installed { .. })
        );
        Self { was_installed }
    }
}

impl Drop for Restore {
    fn drop(&mut self) {
        // Leave the machine as we found it: remove what the test installed, or
        // restore the developer's own installation.
        let _ = Platform::uninstall();
        if self.was_installed
            && let Ok(exe) = std::env::current_exe()
        {
            let _ = Platform::install(&exe);
        }
    }
}

/// A path that classifies as a durable install location, so the guard against
/// registering volatile paths does not reject it.
fn stable_exe() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir()
            .expect("local data dir")
            .join("Programs")
            .join("mcopy")
            .join("mcopy.exe")
    }
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/Applications/mcopy.app/Contents/MacOS/mcopy")
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        PathBuf::from("/usr/bin/mcopy")
    }
}

#[test]
fn install_then_uninstall_leaves_no_trace() {
    let _guard = serial();
    let _restore = Restore::capture();

    Platform::uninstall().expect("start from a clean slate");

    Platform::install(&stable_exe()).expect("install must not need elevation");
    assert!(
        matches!(
            Platform::state().expect("read state"),
            ContextMenuInstallState::Installed { .. }
        ),
        "the integration should report itself installed"
    );

    Platform::uninstall().expect("uninstall must not need elevation");
    assert_eq!(
        Platform::state().expect("read state"),
        ContextMenuInstallState::NotInstalled,
        "uninstall must remove every trace"
    );
}

#[test]
fn a_fresh_install_reports_the_current_version() {
    let _guard = serial();
    let _restore = Restore::capture();

    Platform::uninstall().expect("clean slate");
    platform::install_or_update_context_menu(&stable_exe()).expect("install");

    let state = Platform::state().expect("read state");
    assert!(
        state.is_current_version(),
        "a fresh install should be marked with the running version, got {state:?}"
    );
}

#[test]
fn installing_twice_is_idempotent() {
    let _guard = serial();
    let _restore = Restore::capture();

    Platform::uninstall().expect("clean slate");
    let exe = stable_exe();

    platform::install_or_update_context_menu(&exe).expect("first install");
    // The second call short-circuits on the version marker; it must not fail
    // or leave a partially-rewritten integration behind.
    platform::install_or_update_context_menu(&exe).expect("second install");

    assert!(Platform::state().expect("read state").is_current_version());
}

#[test]
fn uninstalling_when_nothing_is_installed_succeeds() {
    let _guard = serial();
    let _restore = Restore::capture();

    Platform::uninstall().expect("first uninstall");
    // Idempotent: removing an absent integration is a no-op, not an error.
    Platform::uninstall().expect("second uninstall");
    assert_eq!(
        Platform::state().expect("read state"),
        ContextMenuInstallState::NotInstalled
    );
}

/// Registering menu entries that embed a path which is about to disappear was
/// the silent failure at the heart of issue 3.
#[test]
fn installing_from_a_volatile_path_is_refused() {
    let _guard = serial();
    let _restore = Restore::capture();

    let volatile = std::env::temp_dir().join("mcopy-download").join("mcopy");
    let error = platform::install_or_update_context_menu(&volatile)
        .expect_err("a temp-directory path must be rejected");

    let message = error.to_string();
    assert!(
        message.contains("will not persist"),
        "the error should explain the problem, got: {message}"
    );
}

/// Issue 6: the Paste entry must only be offered when pasting would do
/// something. macOS has no runtime mechanism for this and reports
/// `Unsupported`, which is a documented limitation rather than a failure.
#[test]
fn paste_visibility_can_be_toggled_where_the_platform_allows_it() {
    let _guard = serial();
    let _restore = Restore::capture();

    Platform::uninstall().expect("clean slate");
    Platform::install(&stable_exe()).expect("install");

    let hidden = Platform::set_paste_available(false)
        .expect("hiding the paste entry must not fail");
    let shown = Platform::set_paste_available(true)
        .expect("showing the paste entry must not fail");

    if cfg!(target_os = "macos") {
        assert_eq!(hidden, PasteVisibility::Unsupported);
        assert_eq!(shown, PasteVisibility::Unsupported);
    } else {
        assert_eq!(hidden, PasteVisibility::Applied);
        assert_eq!(shown, PasteVisibility::Applied);
    }

    // Toggling visibility must never disturb the install itself.
    assert!(Platform::state().expect("read state").is_current_version());
}

#[test]
fn toggling_paste_visibility_without_an_install_is_harmless() {
    let _guard = serial();
    let _restore = Restore::capture();

    Platform::uninstall().expect("clean slate");
    // Nothing is installed, so there is no menu to gate. This must be a quiet
    // no-op: copy and paste have to keep working regardless.
    Platform::set_paste_available(true).expect("no install, no error");
    Platform::set_paste_available(false).expect("no install, no error");
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use winreg::RegKey;
    use winreg::enums::*;

    const PASTE_KEYS: &[&str] = &[
        r"Software\Classes\Directory\Background\shell\mcopy_paste",
        r"Software\Classes\Directory\shell\mcopy_paste",
        r"Software\Classes\Drive\shell\mcopy_paste",
    ];
    const COPY_KEYS: &[&str] = &[
        r"Software\Classes\*\shell\mcopy_copy",
        r"Software\Classes\Directory\shell\mcopy_copy",
    ];

    fn paste_hidden(path: &str) -> bool {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(path, KEY_READ)
            .ok()
            .is_some_and(|key| {
                key.get_value::<String, _>("LegacyDisable").is_ok()
            })
    }

    /// The whole privilege fix: these keys live under HKCU, so no step of the
    /// normal flow can require administrator rights.
    #[test]
    fn the_integration_is_written_under_the_current_user() {
        let _guard = serial();
        let _restore = Restore::capture();

        Platform::uninstall().expect("clean slate");
        Platform::install(&stable_exe()).expect("install");

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        for path in COPY_KEYS.iter().chain(PASTE_KEYS) {
            assert!(
                hkcu.open_subkey_with_flags(*path, KEY_READ).is_ok(),
                "{path} was not created under HKCU"
            );
        }
    }

    #[test]
    fn the_paste_verbs_start_hidden_and_follow_the_copy_state() {
        let _guard = serial();
        let _restore = Restore::capture();

        Platform::uninstall().expect("clean slate");
        Platform::install(&stable_exe()).expect("install");

        // Fresh install: nothing has been copied, so Paste must be hidden.
        for path in PASTE_KEYS {
            assert!(
                paste_hidden(path),
                "{path} should start hidden on a fresh install"
            );
        }

        Platform::set_paste_available(true).expect("show");
        for path in PASTE_KEYS {
            assert!(
                !paste_hidden(path),
                "{path} should be visible after a copy"
            );
        }

        Platform::set_paste_available(false).expect("hide");
        for path in PASTE_KEYS {
            assert!(
                paste_hidden(path),
                "{path} should be hidden again after the paste completes"
            );
        }
    }

    /// Copy is what creates the state, so it must never be gated.
    #[test]
    fn the_copy_verbs_are_never_hidden() {
        let _guard = serial();
        let _restore = Restore::capture();

        Platform::uninstall().expect("clean slate");
        Platform::install(&stable_exe()).expect("install");
        Platform::set_paste_available(false).expect("hide paste");

        for path in COPY_KEYS {
            assert!(
                !paste_hidden(path),
                "{path} must stay visible whatever the copy state"
            );
        }
    }

    #[test]
    fn the_registered_command_points_at_the_installed_executable() {
        let _guard = serial();
        let _restore = Restore::capture();

        Platform::uninstall().expect("clean slate");
        let exe = stable_exe();
        Platform::install(&exe).expect("install");

        let command: String = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(
                r"Software\Classes\Directory\Background\shell\mcopy_paste\command",
                KEY_READ,
            )
            .expect("command key")
            .get_value("")
            .expect("command value");

        assert!(
            command.contains(exe.to_str().unwrap()),
            "the verb should invoke the installed binary, got: {command}"
        );
        assert!(command.starts_with('"'), "the path must be quoted");
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;

    fn home() -> PathBuf {
        dirs::home_dir().expect("home directory")
    }

    fn nautilus_paste_script() -> PathBuf {
        home().join(".local/share/nautilus/scripts/mcopy-paste")
    }

    fn dolphin_service() -> PathBuf {
        home().join(".local/share/kio/servicemenus/mcopy.desktop")
    }

    #[test]
    fn the_paste_affordances_follow_the_copy_state() {
        let _guard = serial();
        let _restore = Restore::capture();

        Platform::uninstall().expect("clean slate");
        Platform::install(&stable_exe()).expect("install");

        // Fresh install: nothing copied yet.
        assert!(
            !nautilus_paste_script().exists(),
            "the Nautilus paste script should not exist before a copy"
        );
        let service = std::fs::read_to_string(dolphin_service()).unwrap();
        assert!(!service.contains("mcopy_paste"));

        Platform::set_paste_available(true).expect("show");
        assert!(nautilus_paste_script().exists());
        let service = std::fs::read_to_string(dolphin_service()).unwrap();
        assert!(service.contains("[Desktop Action mcopy_paste]"));

        Platform::set_paste_available(false).expect("hide");
        assert!(!nautilus_paste_script().exists());
        let service = std::fs::read_to_string(dolphin_service()).unwrap();
        assert!(!service.contains("[Desktop Action mcopy_paste]"));
    }

    #[test]
    fn uninstall_removes_every_integration_file() {
        let _guard = serial();
        let _restore = Restore::capture();

        Platform::install(&stable_exe()).expect("install");
        Platform::set_paste_available(true).expect("show paste");
        Platform::uninstall().expect("uninstall");

        assert!(!nautilus_paste_script().exists());
        assert!(!dolphin_service().exists());
        assert!(
            !home()
                .join(".local/share/nautilus/scripts/mcopy-copy")
                .exists()
        );
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    fn services_dir() -> PathBuf {
        dirs::home_dir().expect("home").join("Library/Services")
    }

    #[test]
    fn the_workflow_bundles_are_well_formed() {
        let _guard = serial();
        let _restore = Restore::capture();

        Platform::uninstall().expect("clean slate");
        Platform::install(&stable_exe()).expect("install");

        for name in ["Copy with mcopy", "Paste with mcopy"] {
            let bundle = services_dir().join(format!("{name}.workflow"));
            let info = bundle.join("Contents/Info.plist");
            let document = bundle.join("Contents/Resources/document.wflow");

            assert!(info.is_file(), "{name}: Info.plist missing");
            assert!(document.is_file(), "{name}: document.wflow missing");

            // plutil is the authority on whether Finder can read these.
            for plist in [&info, &document] {
                let status = std::process::Command::new("plutil")
                    .arg("-lint")
                    .arg(plist)
                    .status()
                    .expect("run plutil");
                assert!(
                    status.success(),
                    "{}: malformed property list",
                    plist.display()
                );
            }
        }
    }
}
