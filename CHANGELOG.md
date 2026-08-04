# Changelog

All notable user-facing changes to this project are documented in this file.

## [Unreleased]

## [0.3.0] - 2026-08-04

Linux joins Windows and macOS as a supported platform, and mcopy now installs
like a normal desktop application instead of running from wherever it was
downloaded.

### Added

- Native installers for every platform: an Inno Setup installer on Windows, a
  `.pkg` and `.dmg` on macOS, and a `.deb` plus portable tarball on Linux. The
  downloaded file is no longer needed once installation finishes.
- Linux is now built, packaged and tested in CI alongside Windows and macOS.
- The logo follows the system theme: its black bars turn white in dark mode and
  back to black in light mode, updating live when the theme changes. The green
  section never changes color.
- Failed copies now say *why* — permission denied, out of space, read-only
  destination, missing source — with a platform-specific next step instead of a
  bare failure count.
- A message window explains refusals that previously did nothing at all, such as
  pasting with an empty clipboard or into an unwritable folder.
- A log file under the platform's standard location, controlled by `MCOPY_LOG`.
  It records paths and error kinds, never file contents.
- `mcopy status` prints what is currently copied.
- The destination is validated before a paste begins: it must exist or be
  creatable, be a directory, be writable, and not sit inside a copied source.

### Changed

- **The Windows context menu is now registered per-user under `HKEY_CURRENT_USER`
  instead of machine-wide under `HKEY_LOCAL_MACHINE`.** Installing and removing
  the integration no longer requires administrator rights at any point.
- `install` and `uninstall` are now `shell-install` and `shell-uninstall`, since
  application installation is the installer's job. The old names still work.
- The copy progress window and the setup window are ordinary application windows,
  so they appear in the taskbar, Dock and window switcher and can be minimized
  and restored during a long copy.
- Copy and paste state is a single, atomically written, validated record instead
  of three loosely coupled files.
- A paste that finds nothing to do now explains itself instead of exiting
  quietly.
- The progress window no longer closes itself when items failed, so the reason
  stays readable.
- macOS Finder Services now use `copy --append`, matching the other platforms
  for multi-item selections.
- Thunar support was removed. It was never actually installed — the previous
  version only printed setup instructions to a stream nobody could see.

### Fixed

- **The Paste entry no longer appears when nothing has been copied.** It is
  hidden until a copy happens, and hidden again as soon as a paste succeeds.
  Deleting the copied source also hides it. (Windows and Linux; macOS Services
  cannot be toggled at runtime — see Notes.)
- **The copy state is now cleared after a successful paste.** Previously nothing
  ever cleared it, so Paste stayed armed indefinitely, including across reboots.
- **Pasting into a drive root no longer fails with a path syntax error.**
  Explorer expands the verb to `paste "C:\"`, whose trailing separator escapes
  the closing quote and yields the malformed argument `C:"`.
- **The setup window closes on the first click.** Closing is now a single path:
  the previous version raced `PostQuitMessage` against pending input, silently
  vetoed the OS close button for the whole duration of an install, and left the
  close button disabled with no way out.
- The macOS app bundle no longer declares `LSUIElement`, which had made mcopy an
  accessory application with no Dock icon at all.
- Install and uninstall workers are now joined during shutdown instead of being
  detached, so no thread outlives its window.
- Elevated child processes are gone from the normal flow. The previous version
  launched a hidden elevated helper and waited on it forever, reporting every
  failure as an unexplained exit code.
- A cancelled copy can no longer be relabelled as completed by a late
  transition.
- Two rapid pastes no longer race into the same folder; the second is refused.
- Pasting a folder into itself is detected and refused.
- The copy-session window no longer underflows when the system clock steps
  backwards.
- Executable paths written into shell scripts, `.desktop` files and Automator
  workflows are now escaped for their target syntax.
- Printing no longer aborts the process when its output stream has been closed.
- mcopy refuses to register menu entries while running from a download folder,
  disk image, temporary directory or build directory, since those entries would
  break as soon as the location disappeared.
- The unused `mcopy.manifest` was removed. It was never embedded, so its
  `asInvoker` declaration described nothing.

### Notes

- Existing 0.2 machine-wide registry entries are removed automatically when
  mcopy happens to run elevated; otherwise run
  `mcopy shell-uninstall --all-users` from an elevated prompt once. Leftovers are
  harmless — the per-user entries take precedence.
- The 0.2 clipboard payload is discarded on first run rather than migrated,
  because it carries no session or timestamp to validate. Copy once after
  upgrading.
- macOS keeps the Paste service permanently listed: `NSServices` visibility is
  static, and toggling it would mean rewriting the workflow bundle and restarting
  the pasteboard server. Pasting with nothing copied now shows an explanation.
- Installers are not code-signed or notarized, so Windows SmartScreen and macOS
  Gatekeeper will warn on first launch.

## [0.2.0] - 2026-06-29

### Added

- Automated GitHub release publishing from `v*` tags.
- Release helper script for creating and pushing version tags safely.
- Changelog file for tracking user-facing release changes.
- Changelog helper script for drafting release entries from git commit subjects.
- macOS release packaging as a proper `.app` bundle inside a zip archive.
- Bundled Inter font for more consistent UI layout across platforms.

### Changed

- File discovery and copy planning now do more work concurrently.
- Progress updates are now event-driven instead of relying on fixed polling.
- Paused copy workers now wait on notifications instead of polling.
- Progress counters use cheaper atomic updates in hot paths.
- Clipboard append behavior now avoids duplicate entries more efficiently.
- The codebase is split into clearer clipboard, copy, platform, CLI, and UI modules.

### Fixed

- Clipboard session files now live in a private per-user directory.
- Linux paste can survive process exit by persisting copied paths to a session file.
- KDE service menu installation now covers both Plasma 5 and Plasma 6 paths.
- macOS Finder Services are refreshed after install.
- Home directory resolution now uses platform-aware user directory lookup.

### Notes

- GitHub Release notes are read from this changelog's matching version section.
- Windows and macOS builds are attached to the GitHub Release as zip files.
- macOS packages are not signed or notarized yet.
