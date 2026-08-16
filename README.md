# mcopy

Fast, queue-aware file and folder copying built around the native right-click
workflow, on Windows, macOS and Linux.

`mcopy` makes large copy operations feel lighter and more controlled. Instead of
forcing a terminal-first flow, it turns the familiar copy/paste gesture in your
file manager into an async pipeline with a live progress window, cooperative
pause/resume/cancel controls, and a clean separation between shell integration,
copy state, copy orchestration, and UI.

## Contents

- [What mcopy Is](#what-mcopy-is)
- [Install](#install)
- [Using It](#using-it)
- [Commands](#commands)
- [How It Works](#how-it-works)
- [Permissions](#permissions)
- [Architecture](#architecture)
- [Development](#development)
- [Publishing to package registries](#publishing-to-package-registries)
- [Upgrading from 0.2](#upgrading-from-02)
- [Troubleshooting](#troubleshooting)
- [Limitations](#limitations)

## What mcopy Is

A Rust copy tool focused on three things:

- A file-manager-native copy/paste experience
- Async, concurrent file copying
- Visibility and control during long-running operations

The flow is:

1. Select files or folders.
2. Choose **Copy with mcopy**.
3. Navigate to the destination.
4. Choose **Paste with mcopy**.
5. Watch and control the operation in a dedicated progress window.

The **Paste** entry only appears once something has actually been copied, and
disappears again as soon as the paste completes.

## Install

Download the artifact for your platform from the
[latest release](https://github.com/NAKAMOZ/mcopy/releases) and run it. **Once
installation finishes, the downloaded file is no longer needed** — you can delete
it, eject the disk image, or remove the package file, and mcopy will keep
working.

### Windows

Run `mcopy-setup-<version>-x86_64.exe`.

Installs per-user into `%LOCALAPPDATA%\Programs\mcopy`, adds a Start menu entry,
and registers itself in **Settings ▸ Apps** for normal removal. **No
administrator rights are required at any point.** The installer registers the
right-click entries for you — untick that task during setup if you would rather
it did not, and use `mcopy shell-install` later if you change your mind.

The installer is unsigned, so SmartScreen will warn on first run. Choose
*More info ▸ Run anyway*.

### macOS

Open `mcopy-<version>.dmg` and drag **mcopy** to Applications, or run
`mcopy-<version>.pkg` for a guided install.

The build is unsigned and un-notarized, so Gatekeeper will refuse the first
launch. Right-click the app and choose **Open**, then confirm. To remove mcopy,
drag it to the Trash — run `mcopy shell-uninstall` first if you want the Finder
Services removed too.

### Linux

Download `mcopy-<version>-x86_64.AppImage`, make it executable and run it — it
works on any distribution, no installation or root access required:

```bash
chmod +x mcopy-<version>-x86_64.AppImage
./mcopy-<version>-x86_64.AppImage
```

Alternatively, use the portable tarball to install into `~/.local`:

```bash
tar xzf mcopy-<version>-x86_64.tar.gz
cd mcopy-<version>
./install.sh                 # installs into ~/.local, no root needed
```

The file-manager entries register themselves: the tarball's `install.sh` does it
as it installs, and the AppImage does it the first time you run it. Removal is
`mcopy shell-uninstall` followed by deleting the AppImage, or `./uninstall.sh`
for a tarball install.

Nautilus (GNOME) and Dolphin (KDE) are supported. Thunar is not — see
[Limitations](#limitations).

### Building from source

```bash
cargo build --release
```

On Linux you will need the development packages gpui links against:

```bash
sudo apt install libx11-dev libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev \
                 libwayland-dev libfontconfig1-dev libfreetype6-dev \
                 libvulkan-dev libssl-dev pkg-config
```

## Using It

Select files or folders, right-click, and choose **Copy with mcopy**. Multi-item
selections are supported: the file manager invokes the verb once per item, and
mcopy stitches them into a single copy session.

Navigate to the destination, right-click, and choose **Paste with mcopy**. A
progress window opens showing the current file, an overall counter, and Pause and
Cancel controls. It appears in the taskbar, Dock or window switcher, so you can
minimize it and come back to it during a long copy.

When the copy finishes cleanly the window closes itself. If any items failed it
stays open and names the reason.

## Commands

mcopy is usable from a terminal as well as from the file manager.

| Command | Purpose |
| --- | --- |
| `mcopy` | Register the file-manager entries, check for updates, exit |
| `mcopy shell-install` | Register the file-manager entries for the current user |
| `mcopy shell-uninstall` | Remove them |
| `mcopy shell-uninstall --all-users` | Also remove machine-wide entries left by 0.2 (Windows; needs admin) |
| `mcopy copy <paths…>` | Copy paths into the mcopy clipboard |
| `mcopy copy --append <paths…>` | Add to the current copy session |
| `mcopy paste <target>` | Paste the copied items into `target` |
| `mcopy status` | Show what is currently copied |
| `mcopy clear` | Forget the copied items |
| `mcopy <src> <dst>` | Direct terminal copy with progress bars |

`install` and `uninstall` remain as aliases for `shell-install` and
`shell-uninstall`.

Terminal-copy options: `-j/--concurrency <n>` to override the worker count, and
`--no-progress` to suppress the progress bars.

## How It Works

1. **Copy** canonicalizes the selection and records it as a copy session.
2. The Paste entries in the file manager become visible.
3. **Paste** validates the destination, then expands the selection into a flat
   copy plan.
4. Destination directories are created up front.
5. The progress window opens and the queue runs concurrently.
6. On success the copy session is consumed and the Paste entries hide again.

### Copy state

The copy session is a single record in a private per-user directory, written
atomically. It is the source of truth; the system clipboard is written too, but
only for interoperability. That split matters on Linux, where the X11/Wayland
selection belongs to a live process and would vanish the moment the short-lived
`copy` command exits.

The session is validated every time it is read, so sources deleted since the copy
are dropped automatically — and if none survive, the Paste entries hide.

A session is consumed only by a paste that fully succeeds. Cancelled, failed and
partially failed pastes keep it, so you can simply try again.

Concurrent pastes are prevented by a lock: a second paste started while one is
running is refused rather than allowed to race into the same folder.

### Concurrency

Default is CPU cores × 4, clamped to between 4 and 128.

### Updates

Launching `mcopy` on its own checks GitHub for a newer release, at most once a
day. The copy and paste commands never check — they are invoked once per item by
the file manager and must not wait on the network.

If there is a new version, mcopy asks before doing anything. Accepting downloads
the artifact for your platform and verifies it against the release's
`SHA256SUMS` before running it; a download that does not match is deleted and
nothing is installed. From there Windows and macOS hand off to their own
installers, and the AppImage is replaced in place and used from the next launch.

A tarball install spreads files across a prefix rather than being one file to
swap, so it is pointed at the releases page instead.

## Permissions

**mcopy never requires administrator or root rights for normal operation.**

Shell integration is per-user everywhere: `HKEY_CURRENT_USER\Software\Classes` on
Windows, `~/Library/Services` on macOS, and `~/.local/share` on Linux. The one
exception is `shell-uninstall --all-users`, which exists purely to clean up the
machine-wide entries that mcopy 0.2 created and which does need elevation.

If a copy hits a permission problem, the progress window names it and suggests a
next step, rather than reporting an anonymous failure count. On macOS a denial in
`~/Desktop`, `~/Documents` or `~/Downloads` is usually a privacy (TCC)
restriction, and the window says so.

### Logging

mcopy writes a log to:

| Platform | Location |
| --- | --- |
| Windows | `%LOCALAPPDATA%\mcopy\logs\mcopy.log` |
| macOS | `~/Library/Logs/mcopy/mcopy.log` |
| Linux | `$XDG_STATE_HOME/mcopy/mcopy.log` (usually `~/.local/state`) |

Set `MCOPY_LOG` to `debug`, `info`, `warn`, `error` or `off`. It records
filesystem paths and error kinds; it never records file contents or clipboard
payloads.

## Architecture

```text
src/
├── main.rs              command routing; owns the Tokio runtime
├── lib.rs               public surface
├── cli/                 argument parsing and command implementations
├── clipboard/
│   ├── mod.rs           copy/paste API and the system-clipboard mirror
│   └── state.rs         the owned copy-state model, lock, and on-disk format
├── copy/
│   ├── mod.rs           the concurrent copy engine
│   ├── controller.rs    cooperative pause / resume / cancel
│   ├── error.rs         failure classification
│   ├── progress.rs      progress event types
│   └── walk.rs          directory traversal and copy planning
├── platform/
│   ├── mod.rs           the ContextMenu trait — the single #[cfg] seam
│   ├── location.rs      is this executable somewhere durable?
│   ├── windows.rs       Explorer verbs (HKCU)
│   ├── macos.rs         Finder Services
│   └── linux.rs         Nautilus scripts and Dolphin service menus
├── ui/
│   ├── theme.rs         light/dark palettes
│   ├── widgets.rs       shared elements, including the logo
│   ├── shutdown.rs      the single shutdown path
│   ├── progress/        the copy progress window
│   ├── update_prompt.rs the update-available window
│   └── notice.rs        one-message window
├── update/
│   ├── mod.rs           the release check, download and verification
│   ├── cache.rs         the once-a-day throttle
│   ├── github.rs        the Releases API client
│   ├── asset.rs         artifact selection and checksum matching
│   └── installer.rs     the UpdateInstaller trait — the single #[cfg] seam
└── util/                paths, shell escaping, logging, safe output
```

Platform differences live behind the `ContextMenu` trait, selected in exactly one
place in `platform/mod.rs`. Nothing else in the codebase branches on the OS.

### Project identity

Every installer, package and registry manifest has to agree on who publishes
mcopy and what it is called. Those values are declared once and read by
everything else:

| Value | Declared in |
| --- | --- |
| App id `io.github.nakamoz.mcopy` | `mcopy::APP_ID` in `src/lib.rs` |
| Publisher `NAKAMOZ` | `mcopy::APP_PUBLISHER` |
| Copyright | `mcopy::APP_COPYRIGHT` |
| Version, description, homepage, license, author | `Cargo.toml` `[package]` |

`scripts/identity.sh` and `scripts/Identity.ps1` parse those two files, and every
packaging script sources one of them. `build.rs` carries its own copy of the
publisher and copyright (a build script cannot depend on the crate it builds) and
exports what it embedded, which a unit test in `src/lib.rs` asserts against the
constants — so the shipped binary's File Properties can never disagree with the
AppImage, the `.pkg` or the winget manifest.

The app id is load-bearing rather than decorative. It is simultaneously the macOS
`CFBundleIdentifier` and installer package id, the Wayland/X11 `app_id`, the
`.desktop` file's name, the AppStream component id, and the Finder Services
bundle prefix. A mismatch between any two shows up as a window with no icon or an
application a software centre cannot attribute.

## Publishing to package registries

The release workflow produces the installers; the registry manifests are
generated next to them so their checksums always match the artifact being
described.

**Winget** — after publishing the GitHub release:

```powershell
.\scripts\package-windows.ps1        # produces dist\mcopy-setup-<version>-x86_64.exe
.\scripts\package-winget.ps1         # produces dist\winget\manifests\...
winget validate --manifest dist\winget\manifests\n\NAKAMOZ\mcopy\<version>
```

Copy that directory into a fork of
[microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) and open a pull
request. The `InstallerUrl` must already resolve, so publish the release first.

**Homebrew** — `scripts/package-macos.sh` writes `dist/mcopy.rb` alongside the
disk image, with that image's SHA256 already filled in:

```bash
brew audit --cask --new --online dist/mcopy.rb
```

Submit it to [homebrew-cask](https://github.com/Homebrew/homebrew-cask), or host
it in a personal tap.

**Linux distributions** — the AppImage bundles an AppStream component and
desktop entry so GNOME Software, KDE Discover and AppImage catalogues like
[AppImageHub](https://appimage.github.io) can show the developer, description
and license. `scripts/package-linux.sh` validates both the AppStream metadata
and the desktop entry when `appstreamcli` and `desktop-file-validate` are
available; CI installs them so validation always runs there.

### Tech stack

Rust 2024 · Tokio · Futures · Clap · Indicatif · GPUI · arboard · dirs

## Development

```bash
cargo fmt --all
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

### Checking before you push

`scripts/preflight.sh` runs what CI runs, in about three seconds against a warm
`target/`:

```bash
./scripts/preflight.sh          # fmt, clippy, tests, CLI smoke test
./scripts/preflight.sh --full   # also builds and packages, and runs the AppImage
```

It keeps going after a failure, so one run reports every problem rather than
making you fix and re-run.

What it cannot cover is **Windows and macOS**. CI builds on all three, and the
`#[cfg(target_os)]` code behind those never compiles here. Cross-checking is not
available either: `ring`, pulled in by gpui's HTTP client, compiles C for the
target, so `cargo check --target x86_64-pc-windows-msvc` wants MSVC's `lib.exe`
and the Apple target wants an osxcross toolchain — a rustup target alone is not
enough. Treat a green preflight as "the Linux job will pass", nothing more.

To run the workflow files themselves, use [act](https://github.com/nektos/act),
which executes GitHub Actions in Docker:

```bash
act -j check -P ubuntu-latest=catthehacker/ubuntu:act-latest
```

Worth it when you have edited a workflow and want to know the YAML is right.
Not worth it as a routine pre-push check: act only emulates the Linux runners,
gets no `Swatinem/rust-cache`, and reinstalls the apt build dependencies and
recompiles gpui from scratch each run — minutes, against preflight's seconds.

Packaging:

```powershell
.\scripts\package-windows.ps1        # needs: choco install innosetup
```

```bash
./scripts/package-macos.sh           # macOS only; uses built-in tools
./scripts/package-linux.sh           # needs appimagetool (auto-downloaded if missing)
```

Bumping the version means editing `Cargo.toml` only — the Windows version
resource, the Inno installer, the `.app` bundle, the AppImage filename and the
registry manifests all read it from there.

Manual test coverage that CI cannot reach is listed in
[docs/VALIDATION.md](docs/VALIDATION.md).

### Releasing

Bump `version` in `Cargo.toml`, add a `CHANGELOG.md` section for it, commit, then:

```bash
scripts/release.sh                   # or --dry-run to preview
```

Pushing the tag triggers GitHub Actions to run the test suite on all three
platforms, build the installers, and publish the release using the changelog
section as the release notes.

## Upgrading from 0.2

- **Run the new installer.** 0.3 refuses to register menu entries while running
  from a download folder, disk image, temporary directory or build directory,
  because those entries break as soon as the location disappears.
- **Windows entries moved from `HKEY_LOCAL_MACHINE` to `HKEY_CURRENT_USER`.**
  0.3 removes the old machine-wide keys when it happens to run elevated. If it
  cannot, run `mcopy shell-uninstall --all-users` once from an elevated prompt.
  Leftovers are harmless; the per-user entries take precedence.
- **Menus are now per-user.** Other accounts on the machine register their own.
- **Copy once after upgrading.** The 0.2 clipboard payload is discarded rather
  than migrated, because it carries no session or timestamp to validate.
- **Paste now clears after a successful paste.** This is the point of the
  release, but it is a visible behavior change if you relied on pasting the same
  selection repeatedly. Copy again to paste again.
- **macOS: reinstall the app bundle.** 0.3 removes `LSUIElement`, which had
  suppressed the Dock icon entirely. The bundle identifier also changed from
  `com.mcopy.app` to `io.github.nakamoz.mcopy`, so macOS treats it as a new
  application and any privacy (Full Disk Access) permission must be granted
  again.
- **`install` / `uninstall` are now `shell-install` / `shell-uninstall`.** The
  old names still work.
- **Thunar users:** support was removed. It never actually worked; the previous
  version only printed instructions to a stream nobody could see.

## Troubleshooting

**The right-click menu does not appear.**
Make sure mcopy is installed rather than being run from your downloads folder —
launching it will say so if it is not. Otherwise run `mcopy shell-install`. On
Windows, entries appear under *Show more options*.

**The Paste entry is missing.**
That is deliberate: it only appears after a successful copy. Run `mcopy status`
to see what is currently copied. It also hides if the copied sources have since
been deleted.

**The menu opens an older build.**
Run `mcopy shell-uninstall`, then install the current version and launch it once.

**Items failed during a copy.**
The progress window names the cause and stays open so you can read it. The log
has the full paths.

**Nothing happens when I click Paste.**
0.3 always shows a window explaining why a paste cannot start. If you see nothing
at all, check the log.

**macOS says the app is damaged or from an unidentified developer.**
The build is unsigned. Right-click the app and choose Open, or run
`xattr -dr com.apple.quarantine /Applications/mcopy.app`.

## Limitations

- Installers are not code-signed or notarized. Windows SmartScreen and macOS
  Gatekeeper will warn on first launch.
- **macOS keeps the Paste service permanently listed.** `NSServices` visibility
  is static, and toggling it would mean rewriting the workflow bundle and
  restarting the pasteboard server on every copy. Pasting with nothing copied
  shows an explanation instead.
- On Windows 11 the entries appear under *Show more options*, not the top-level
  menu. Top-level placement requires an MSIX-packaged `IExplorerCommand`.
- **Thunar is not supported.**
- Nautilus 43+ de-emphasizes `~/.local/share/nautilus/scripts`. The entries work
  but are less discoverable than a native extension would be.
- gpui requires a working Vulkan driver on Linux. Software rendering (llvmpipe)
  works but is slow.
- Client-side decorations and window transparency render inconsistently across
  Wayland compositors; some tiling window managers ignore taskbar hints.
- The application icon itself is not theme-aware — only the logo drawn inside the
  windows is. Operating systems do not offer a cheap way to swap an app icon per
  theme.
- On Windows the binary is GUI-subsystem, so terminal output is best-effort. It
  is visible when run interactively, but capturing it into a variable from
  PowerShell is unreliable. Scripts should use exit codes.
- Only x86-64 builds are published. Apple Silicon runs the macOS build under
  Rosetta.
