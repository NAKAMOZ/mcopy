# Changelog

All notable user-facing changes to this project are documented in this file.

## [Unreleased]

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
