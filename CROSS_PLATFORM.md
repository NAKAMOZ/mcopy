# mcopy — Cross-Platform Support Plan (macOS · Windows · Linux)

What already works across all three platforms, the real gaps, and the concrete changes to close them. mcopy is *already* mostly portable — the dependencies (`gpui`, `arboard`, `clap`, `tokio`) are cross-platform and the OS integration is `#[cfg]`-gated. The risks below are the parts that compile everywhere but **misbehave** on a specific OS.

---

## Already portable (no change needed)

| Concern | Why it's fine |
|---------|---------------|
| Copy engine ([src/lib.rs](src/lib.rs)) | Pure `tokio::fs` — identical on all three. |
| UI framework ([src/ui/](src/ui/)) | `gpui` supports macOS, Windows, and Linux (X11 + Wayland). |
| CLI parsing | `clap` is platform-agnostic. |
| Build script ([build.rs](build.rs)) | Windows resource embed is correctly `#[cfg(windows)]`-guarded. |
| Context-menu install | Three `#[cfg]` impls exist for Win/macOS/Linux. |
| Temp session file ([clipboard.rs](src/clipboard.rs)) | Uses `std::env::temp_dir()` — portable. |

So this is **not** a port from scratch — it is hardening the platform-specific edges.

---

## Critical gaps (compiles, but breaks at runtime)

### C1. Linux clipboard does not survive process exit ⚠️ highest priority

**Where:** [src/clipboard.rs:36-58](src/clipboard.rs#L36-L58)

**The problem:** On Windows and macOS the OS clipboard *owns a copy* of the data — once `set_text` returns, the payload survives the process exiting. On Linux (both X11 and Wayland) the clipboard works by **selection ownership**: the *process that set the clipboard serves the data on request*. mcopy's `copy` command sets the text and immediately exits → on Linux the clipboard is empty by the time the user runs `paste`. The whole copy→paste flow silently fails on Linux.

**Two ways to fix:**

1. **Own temp-file payload (recommended).** Stop round-tripping the path list through the *system* clipboard. Write the newline-separated paths into mcopy's own file (next to the existing session timestamp in `temp_dir()`), and have `paste` read that file. This is fully portable, sidesteps the ownership model entirely, and is actually more robust on all three OSes (no clipboard-manager interference, no text-format quirks). The system clipboard stops being load-bearing.

2. **Daemonize on Linux only.** Use `arboard`'s `SetExtLinux::wait()` so the process forks and keeps serving the selection. Downside: the `copy` process must stay alive in the background — awkward for a context-menu one-shot, and it lingers.

Given mcopy *already* maintains its own temp session file, **option 1 is the natural fit** and removes a class of platform bugs. Keep writing to the system clipboard too if you want interop with normal Ctrl+V of file paths, but treat the temp file as the source of truth for `paste`.

### C2. macOS GUI needs an `.app` bundle to activate properly

**Where:** [src/ui/window.rs:278-304](src/ui/window.rs#L278-L304), [src/ui/install.rs:529-577](src/ui/install.rs#L529-L577)

**The problem:** A bare Mach-O binary launched on macOS is a "background" process — `cx.activate(true)` and proper Dock/focus behavior require a real application bundle with an `Info.plist` (and ideally code-signing). Run as a loose binary, the window may open unfocused or behind other apps, and the app won't show up correctly.

**Do:** Ship a `mcopy.app` bundle layout for macOS releases:
```
mcopy.app/Contents/
├── Info.plist          # CFBundleExecutable, CFBundleIdentifier, LSUIElement, NSHighResolutionCapable
├── MacOS/mcopy         # the binary
└── Resources/logo.icns
```
Automate it in CI (or a `cargo xtask bundle` / `cargo-bundle`). The CLI subcommands still work as a bare binary; only the GUI windows need the bundle.

### C3. Bundled UI font — "Inter" may be absent

**Where:** [src/ui/window.rs:161](src/ui/window.rs#L161), [src/ui/widgets.rs:133](src/ui/widgets.rs#L133), [src/ui/install.rs](src/ui/install.rs) — `.font_family("Inter")`

**The problem:** `Inter` is hardcoded but is not installed by default on Windows or most Linux distros (and not all macOS). When missing, `gpui` falls back to a system font and the carefully-positioned install UI (which uses absolute pixel offsets) can shift or clip.

**Do:** Bundle `Inter` as an asset and register it via the existing `AssetSource` (load the `.ttf`/`.otf` bytes), or pick a guaranteed-present per-OS stack (`Segoe UI` / `SF Pro` / `system-ui`). Bundling is the only way to get identical layout everywhere.

---

## Platform-integration gaps (context menu correctness)

### P1. Linux KDE: `kservices5` is KDE Plasma 5 only

**Where:** [src/context_menu.rs:527](src/context_menu.rs#L527), [src/context_menu.rs:615-642](src/context_menu.rs#L615-L642)

KDE Plasma 6 moved service menus to `~/.local/share/kio/servicemenus/` (and the `.desktop` schema changed slightly). Installing only to `kservices5` means Dolphin on Plasma 6 shows nothing.

**Do:** Write to **both** `kservices5/ServiceMenus` (Plasma 5) and `kio/servicemenus` (Plasma 6); remove both on uninstall. Detect or just install to both — harmless if one is unused.

### P2. Linux Nautilus scripts are deprecated in newer GNOME

Nautilus 43+ de-emphasizes `~/.local/share/nautilus/scripts` in favor of extensions. Scripts still run but are less discoverable.

**Do:** Document the limitation; optionally add a `nautilus-python` extension path later. Low priority — scripts still function.

### P3. Linux Wayland clipboard / window decorations

- `arboard` on Wayland needs the compositor's data-control protocol; if absent it errors. Tie-in with **C1** — the temp-file approach avoids this entirely for the path payload.
- `WindowBackgroundAppearance::Transparent` + `WindowDecorations::Client` ([window.rs:291-293](src/ui/window.rs#L291-L293)) render differently across compositors; some tiling WMs ignore client-side rounding/transparency. Acceptable, but test on GNOME/Wayland, KDE, and a tiling WM.

### P4. macOS Finder Services discoverability

**Where:** [src/context_menu.rs:226-249](src/context_menu.rs#L226-L249)

The generated Automator `.workflow` is dropped into `~/Library/Services` but macOS may not pick it up until `pbs` is refreshed, and the user must enable it under System Settings → Keyboard → Shortcuts → Services. The code already prints that note — good — but consider running `/System/Library/CoreServices/pbs -update` after install to force a refresh.

---

## Minor / hardening

- **Home directory resolution** — macOS/Linux read `std::env::var("HOME")` directly ([context_menu.rs:227](src/context_menu.rs#L227), etc.). Prefer the `dirs` crate (already in the dependency tree transitively) for `dirs::home_dir()` / `dirs::data_local_dir()` — handles edge cases and is consistent with how Windows resolves paths.
- **`println!` under `windows_subsystem = "windows"`** — [main.rs:80](src/main.rs#L80), [main.rs:194-208](src/main.rs#L194-L208) print to a console that does not exist in the GUI Windows build. Harmless (writes are dropped) but dead on Windows; the install/legacy paths that print are CLI-invoked, so keep them but don't rely on them for the GUI flow.
- **Line endings in generated scripts** — the Linux shell scripts and macOS `.wflow` are written with `\n`; correct for those platforms. Just ensure no `\r\n` creeps in if generated on Windows cross-builds (it won't with the current string literals).
- **`is_elevated` / `winreg` / `windows-sys`** — already correctly `#[cfg(windows)]` in [Cargo.toml](Cargo.toml). No change.

---

## Suggested order of work

1. **C1 — Linux clipboard via temp-file payload.** Highest impact: it's a silent functional failure on Linux today. Also simplifies Wayland (P3).
2. **C3 — bundle the Inter font.** Fixes UI consistency on Windows/Linux.
3. **C2 — macOS `.app` bundle.** Needed for a real macOS GUI release.
4. **P1 — KDE Plasma 6 service-menu path.**
5. **P4, P2, P3, minor items** — integration polish and hardening.

---

## Per-platform release checklist

| Step | Windows | macOS | Linux |
|------|---------|-------|-------|
| Build | `cargo build --release` (embeds icon/manifest) | `cargo build --release` + bundle `.app` | `cargo build --release` |
| Clipboard model | OS-owned (works) | OS-owned (works) | **temp-file payload (C1)** |
| GUI activation | works | needs `.app` (C2) | test per compositor (P3) |
| Font | bundle Inter (C3) | bundle Inter (C3) | bundle Inter (C3) |
| Context menu | registry (admin) | Finder Services + `pbs` refresh (P4) | Nautilus + Dolphin 5 **and 6** (P1) + Thunar manual |
| Smoke test | copy → paste via Explorer menu | copy → paste via Finder Service | copy → paste via file-manager action |

---

## Bottom line

mcopy compiles on all three platforms today, and most of the surface is genuinely portable. The one change that actually *unblocks* Linux is **C1** (clipboard ownership). After that, the remaining work is UI consistency (font, macOS bundle) and context-menu reach (KDE 6). None of it requires rewriting the copy engine or the UI — it's edge hardening behind the existing `#[cfg]` and asset seams. Pairs with the trait-based `platform/` module proposed in [MODULARIZATION.md](MODULARIZATION.md), which is the right home for these per-OS branches.
