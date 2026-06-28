# mcopy — Modularization Plan

How to break the code into focused modules and folders so each file has one job, platform code is isolated, and the public library surface is explicit. No behavior changes — pure restructuring.

---

## Why restructure

| File | Lines | Problem |
|------|-------|---------|
| [src/lib.rs](src/lib.rs) | 277 | Grab-bag "core": copy controller, progress types, FS traversal, path util, and the copy engine all in one file. |
| [src/context_menu.rs](src/context_menu.rs) | 687 | Three full platform implementations (Windows/macOS/Linux) inline in one file. |
| [src/main.rs](src/main.rs) | 283 | Mixes CLI definition, command dispatch, the legacy terminal-progress UI, and the admin check. |
| [src/ui/](src/ui/) | — | Already split, but progress *state* lives next to GPUI *view* code. |

The guiding rule: **one responsibility per module**, and **platform-specific code behind a single trait/seam**, not scattered `#[cfg]` blocks.

---

## Target layout

```
src/
├── main.rs                  # thin: parse args, dispatch to commands. Nothing else.
├── lib.rs                   # crate root: `pub mod` re-exports only.
│
├── cli/
│   ├── mod.rs               # Args/Commands (clap derive structs)
│   └── commands.rs          # run_install / run_copy / run_paste / run_legacy handlers
│
├── copy/                    # the file-copy engine (was the heart of lib.rs)
│   ├── mod.rs               # re-exports; copy_files_with_progress
│   ├── controller.rs        # CopyController (pause/cancel)
│   ├── progress.rs          # ProgressPhase, ProgressUpdate, ProgressCallback
│   └── walk.rs              # collect_files, precreate_directories
│
├── clipboard/
│   ├── mod.rs               # copy/append/paste/clear public fns
│   └── session.rs           # timestamp file + session-timeout logic
│
├── platform/                # OS integration behind one seam
│   ├── mod.rs               # trait + cfg-gated re-export of the active impl
│   ├── state.rs             # ContextMenuInstallState (platform-agnostic)
│   ├── windows.rs           # was windows_impl
│   ├── macos.rs             # was macos_impl
│   └── linux.rs             # was linux_impl
│
├── util/
│   └── path.rs              # normalize_path, calculate_concurrency
│
└── ui/
    ├── mod.rs               # re-exports show_progress_window / show_install_window
    ├── assets.rs            # single shared AssetSource (see OPTIMIZATIONS #5)
    ├── theme.rs             # was constants.rs (colors, sizes, ButtonTone)
    ├── widgets.rs           # unchanged
    ├── progress/
    │   ├── mod.rs           # ProgressWindow (view) — was window.rs
    │   └── state.rs         # CopyProgress / snapshot — was ui/progress.rs
    └── install/
        ├── mod.rs           # InstallWindow (view)
        └── state.rs         # InstallRenderState + start_operation worker
```

---

## What moves where

### 1. Split `lib.rs` into `copy/`, `util/`

`lib.rs` today holds five unrelated concerns. Break them out:

- `CopyController` + `wait_until_resumed` → **`copy/controller.rs`**
- `ProgressPhase`, `ProgressUpdate`, `ProgressCallback` → **`copy/progress.rs`**
- `collect_files`, `precreate_directories` → **`copy/walk.rs`**
- `copy_files_with_progress` → **`copy/mod.rs`** (the orchestrator that ties controller + progress + walk together)
- `normalize_path`, `calculate_concurrency` → **`util/path.rs`**

`lib.rs` shrinks to just module declarations and re-exports so `mcopy::CopyController` etc. still resolve:

```rust
// src/lib.rs
pub mod copy;
pub mod clipboard;
pub mod platform;
pub mod util;
pub mod ui;

pub use copy::{
    CopyController, ProgressPhase, ProgressUpdate, ProgressCallback,
    collect_files, precreate_directories, copy_files_with_progress,
};
pub use util::path::{normalize_path, calculate_concurrency};
```

This keeps the existing import paths in `main.rs` working with zero churn at call sites.

### 2. Turn `context_menu.rs` into a `platform/` folder with a trait

The three inline `#[cfg]` modules become three files, fronted by one trait so the rest of the app never sees `#[cfg]`:

```rust
// src/platform/mod.rs
pub trait ContextMenu {
    fn install(exe_path: &Path) -> anyhow::Result<()>;
    fn uninstall() -> anyhow::Result<()>;
    fn state() -> anyhow::Result<ContextMenuInstallState>;
}

#[cfg(target_os = "windows")] pub use windows::WindowsMenu as Platform;
#[cfg(target_os = "macos")]   pub use macos::MacosMenu     as Platform;
#[cfg(target_os = "linux")]   pub use linux::LinuxMenu     as Platform;
```

Callers use `platform::Platform::install(&exe)` — the `#[cfg]` selection lives in exactly one place. The unsupported-platform fallback becomes a default impl. Pairs with **OPTIMIZATIONS #6** (table-driven registry entries) so `windows.rs` shrinks further.

### 3. Pull command handlers out of `main.rs`

The big `match args.command { ... }` arms become functions in `cli/commands.rs`:

```rust
pub async fn run_paste(target: PathBuf) -> anyhow::Result<()> { ... }
pub async fn run_legacy(src: PathBuf, dst: PathBuf, args: &Args) -> anyhow::Result<()> { ... }
pub fn run_install(exe: &Path) -> anyhow::Result<()> { ... }
```

`main.rs` becomes a thin dispatcher:

```rust
match args.command {
    Some(Commands::Install)  => cli::commands::run_install(&current_exe()?)?,
    Some(Commands::Paste { target }) => cli::commands::run_paste(target).await?,
    None => cli::commands::dispatch_default(args).await?,
    ...
}
```

The legacy `indicatif` terminal-progress setup (~50 lines) moves into `run_legacy`, off the hot path of `main`.

### 4. Separate UI *state* from UI *view*

`CopyProgress` (shared mutable state) currently sits in `ui/progress.rs` next to nothing, while its view sits in `ui/window.rs`. Group by feature:

- `ui/progress/state.rs` ← `CopyProgress`, `CopyProgressSnapshot`
- `ui/progress/mod.rs` ← `ProgressWindow`, `VisualState`, `show_progress_window`
- `ui/install/state.rs` ← `InstallRenderState`, `start_operation`, `perform_install/uninstall`, `run_elevated_command`
- `ui/install/mod.rs` ← `InstallWindow`, `InstallVisual`, `show_install_window`

State is now testable without spinning up GPUI.

### 5. Shared assets + theme

- One `ui/assets.rs` replaces the duplicated `ProgressAssets`/`InstallAssets` (**OPTIMIZATIONS #5**).
- Rename `ui/constants.rs` → `ui/theme.rs`. The install window currently re-declares its own `CARD_BG`/`MUTED_TEXT`/etc. ([install.rs:15-23](src/ui/install.rs#L15-L23)) — fold those into the shared theme so colors live in one place.

---

## Boundary / dependency rules

Enforce a one-directional dependency graph so modules stay decoupled:

```
main → cli → { copy, clipboard, platform, ui }
ui   → copy        (reads progress + controller)
copy → util
clipboard → util
platform → (state only)        # no copy/ui deps
```

- `copy/` must **not** depend on `ui/` (engine stays headless and reusable; it already does via the callback seam — preserve that).
- `platform/` must **not** depend on `copy/` or `ui/`.
- `util/` depends on nothing internal (leaf module).

---

## Migration steps (incremental, compiles at every step)

1. **`util/`** — move `normalize_path` + `calculate_concurrency`, add re-exports. Build.
2. **`copy/`** — move controller, progress types, walk, engine into the folder; keep `lib.rs` re-exports identical so no call site changes. Build + run.
3. **`platform/`** — rename file to folder, split the three `#[cfg]` modules into files, introduce the `ContextMenu` trait. Build per-OS (or at least `cargo check --target` for each).
4. **`cli/`** — extract `Args`/`Commands` and the match arms into handlers; thin out `main.rs`. Build + smoke-test each subcommand.
5. **`ui/`** — split state/view, add shared `assets.rs`, rename `constants.rs`→`theme.rs`, dedupe install colors. Build + open both windows.
6. **Cleanup** — `cargo fmt`, `cargo clippy`, delete dead re-exports, update any module docs.

Each step is a self-contained commit. Because `lib.rs` keeps re-exporting the same names, downstream code (and `main.rs`) compiles unchanged until you choose to update import paths.

---

## Payoff

- **No file over ~200 lines**; each maps to one responsibility.
- **Platform code behind one trait** — adding BSD/other support = one new file + one `cfg` line.
- **Headless engine** (`copy/`) and **headless state** (`ui/*/state.rs`) become unit-testable without a GUI or real OS integration.
- **Single source of truth** for paths, theme, and assets — kills the current duplication.
- Plays directly into [OPTIMIZATIONS.md](OPTIMIZATIONS.md) items #5 and #6.
