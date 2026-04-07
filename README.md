# mcopy

Fast, queue-aware file and folder copying built around the native right-click workflow.

`mcopy` is designed to make large copy operations feel lighter and more controlled. Instead of forcing the user into a terminal-first flow, it turns the familiar Explorer copy/paste gesture into an async pipeline with a live GPUI progress window, cooperative pause/resume/cancel controls, and a clean separation between shell integration, clipboard state, copy orchestration, and UI.

## Navigate This README

If you only want to get started, jump to [Quick Start](#quick-start).

- [What mcopy Is](#what-mcopy-is)
- [Why It Exists](#why-it-exists)
- [Highlights](#highlights)
- [Quick Start](#quick-start)
- [How It Works](#how-it-works)
- [Execution Flow](#execution-flow)
- [Implementation Path](#implementation-path)
- [Architecture](#architecture)
- [Commands](#commands)
- [Usage Guide](#usage-guide)
- [Configuration and Behavior](#configuration-and-behavior)
- [Project Structure](#project-structure)
- [Development](#development)
- [Troubleshooting](#troubleshooting)

## What mcopy Is

`mcopy` is a Rust-based copy tool that focuses on three things:

- A Windows Explorer-friendly copy/paste experience
- Async, concurrent file copying
- Better visibility and control during long-running operations

The main user experience is simple:

1. Select files or folders.
2. Choose `Copy with mcopy`.
3. Navigate to the destination.
4. Choose `Paste with mcopy` or `Paste here with mcopy`.
5. Watch and control the operation in a dedicated progress window.

Under the hood, the app uses the clipboard as a lightweight transfer contract, collects the final file list before copying, pre-creates directories, and then processes the queue concurrently.

## Why It Exists

Traditional copy flows are easy to start but hard to observe and control once they become large. `mcopy` aims to keep the low-friction feel of a right-click workflow while adding the things power users usually want:

- More predictable queue handling
- Faster throughput through concurrency
- A visible, focused progress surface
- Pause, resume, and cancel controls that do not require redesigning the copy engine mid-operation

## Highlights

- Async and concurrent copy pipeline powered by Tokio and Futures
- Windows Explorer context menu integration for copy and paste
- Multi-selection support using clipboard append behavior
- GPUI progress window with live status and action controls
- Cooperative pause, resume, and cancel flow
- Legacy CLI mode for terminal usage
- Clear separation between shell integration, clipboard handling, core engine, and UI

## Quick Start

### 1. Build the release binary

```powershell
cargo build --release
```

Generated executable:

```text
target\release\mcopy.exe
```

### 2. Install the Windows context menu

Open PowerShell as Administrator in the project directory and run:

```powershell
.\target\release\mcopy.exe install
```

You can also run the same flow through Cargo:

```powershell
cargo run --release -- install
```

Installed right-click entries:

- `Copy with mcopy` on files
- `Copy with mcopy` on folders
- `Paste here with mcopy` on folders
- `Paste with mcopy` on folder backgrounds
- `Paste with mcopy` on drive roots

### 3. Use it

1. Select a file or folder in Explorer.
2. Right-click and choose `Copy with mcopy`.
3. Open the target folder.
4. Right-click and choose `Paste with mcopy` or `Paste here with mcopy`.
5. Control the job from the progress window.

### 4. Remove the context menu if needed

```powershell
.\target\release\mcopy.exe uninstall
```

or:

```powershell
cargo run --release -- uninstall
```

## How It Works

At a high level, `mcopy` follows this sequence:

1. The copy action stores selected paths in the clipboard.
2. The paste action reads the clipboard payload and validates the sources.
3. The app recursively expands folders into a flat copy plan.
4. Destination directories are created before file transfer begins.
5. A GPUI progress window is launched on a separate UI thread.
6. The copy engine runs concurrently and emits progress updates.
7. The UI reflects progress and exposes pause, resume, and cancel actions.
8. Once the queue ends, the window briefly shows the terminal state and then auto-closes.

This design keeps the user flow simple while giving the implementation room to remain modular and observable.

## Execution Flow

### Copy phase

When the user triggers `Copy with mcopy`, the application:

- Canonicalizes the selected paths
- Normalizes Windows UNC-prefixed paths when needed
- Stores the result in the clipboard as newline-separated absolute paths
- Supports multi-select by appending into the same clipboard session for a short time window

### Paste phase

When the user triggers `Paste with mcopy`, the application:

- Reads and validates the clipboard payload
- Creates the destination folder if it does not already exist
- Recursively collects every file that should be copied
- Pre-creates destination directories
- Starts the progress window
- Runs the concurrent copy pipeline

### Progress and control phase

During the copy:

- `Pause` stops new work from starting
- `Resume` restarts queue intake
- `Cancel` stops the remaining queue from advancing

Important behavior:

- Pause and cancel are cooperative controls
- Any file copy already in progress is allowed to finish safely
- The queue state updates independently from the UI rendering loop

## Implementation Path

One of the strongest parts of this project is the path it takes architecturally. Instead of building a monolithic “copy app,” `mcopy` follows a layered approach:

### 1. Preserve the native workflow

The project starts from the user’s existing muscle memory: right-click, copy, paste. That keeps the tool approachable and removes the need for a separate launch-first workflow.

### 2. Treat the clipboard as a transport contract

Rather than inventing a heavyweight session manager, the project uses the clipboard to carry canonicalized source paths between the copy and paste phases. This keeps the integration simple and shell-friendly.

### 3. Separate planning from execution

Before any transfer begins, the application collects the full file plan and pre-creates required directories. This makes the execution stage cleaner and easier to observe.

### 4. Keep control cooperative

Instead of trying to interrupt `fs::copy` mid-flight, the control layer pauses or cancels future work. That keeps the implementation simpler and safer while still giving the user meaningful control over the queue.

### 5. Keep UI and engine loosely coupled

The copy engine emits progress updates, and the GPUI layer renders state derived from those updates. This separation keeps the UI replaceable and the core copy logic reusable.

## Architecture

The project is organized around four main concerns.

### Shell and app entry

`src/main.rs` handles:

- CLI parsing
- Context menu install and uninstall commands
- Clipboard-driven paste orchestration
- Legacy terminal mode

### Core copy engine

`src/lib.rs` handles:

- File collection
- Destination directory preparation
- Concurrency selection
- Cooperative control state
- Concurrent file copy execution
- Progress event emission

### Clipboard workflow

`src/clipboard.rs` handles:

- Canonicalizing selected paths
- Storing clipboard payloads
- Multi-select append behavior
- Session timeout behavior
- Payload clearing

### UI layer

`src/ui/` handles:

- Progress state snapshots
- Window lifecycle
- Status rendering
- Buttons and interaction controls
- Terminal-state auto-close behavior

### Context menu integration

`src/context_menu.rs` handles:

- Windows registry integration
- macOS Finder Services integration
- Linux file manager integration helpers

The primary polished workflow is Windows Explorer-first, but the codebase is structured to keep platform-specific integration logic isolated.

## Commands

### Context menu management

```powershell
mcopy install
mcopy uninstall
```

These commands register or remove platform-specific shell integration. On Windows, they require Administrator privileges because they write to the registry.

### Clipboard-oriented commands

```powershell
mcopy copy C:\source\file.txt
mcopy copy --append C:\source\folder
mcopy paste C:\target\folder
mcopy clear
```

These commands are the foundation of the Explorer workflow and can also be used directly from the terminal.

### Legacy CLI mode

```powershell
mcopy C:\source C:\target
mcopy C:\source C:\target -j 16
mcopy C:\source C:\target --no-progress
```

Legacy mode remains useful for terminal-driven workflows and local debugging.

## Usage Guide

### Explorer workflow

1. Select one or more files or folders.
2. Trigger `Copy with mcopy`.
3. Open the destination directory.
4. Trigger `Paste with mcopy` or `Paste here with mcopy`.
5. Monitor and control the operation in the progress window.

### Terminal workflow

Use the CLI directly when you want:

- Scriptable execution
- Legacy progress bars
- Manual concurrency override
- Easier debugging during development

## Configuration and Behavior

### Concurrency

- Default concurrency is `CPU core count x 4`
- The lower bound is `4`
- The upper bound is `128`
- Legacy CLI mode allows manual override with `-j`

### Path behavior

- Source paths are canonicalized before being stored
- Existing clipboard paths are filtered during paste
- Windows UNC prefixes are normalized before use

### Copy semantics

- Directory trees are flattened into a concrete file plan before execution
- Destination directories are created ahead of time
- Progress counts completed and failed files
- Failed files do not stop the whole queue by default

### UI behavior

- The progress window opens as a popup
- Closing the window during an active job triggers cancellation instead of immediate exit
- After completion or cancellation, the window remains visible briefly and then auto-closes

## Project Structure

```text
src/
|- main.rs
|- lib.rs
|- clipboard.rs
|- context_menu.rs
`- ui/
   |- mod.rs
   |- constants.rs
   |- progress.rs
   |- widgets.rs
   `- window.rs
```

Responsibility summary:

- `main.rs`: command routing and application flow
- `lib.rs`: copy planning, concurrency, control, and execution
- `clipboard.rs`: clipboard persistence and session behavior
- `context_menu.rs`: platform-specific shell integration
- `ui/`: GPUI window, state, and presentation logic

## Development

### Tech stack

- Rust 2024 edition
- Tokio
- Futures
- Clap
- Indicatif
- GPUI
- arboard

### Local verification

```powershell
cargo fmt
```

```powershell
cargo check
```

```powershell
cargo clippy --all-targets -- -W unused -W dead_code -W unused_imports
```

### Suggested contributor reading order

If you are new to the codebase, the best reading order is:

1. `src/main.rs`
2. `src/lib.rs`
3. `src/clipboard.rs`
4. `src/context_menu.rs`
5. `src/ui/`

That order mirrors the actual execution path of the product.

## Troubleshooting

### The right-click menu does not appear

- Make sure you ran `install` from an elevated PowerShell session
- Run the install command again
- Restart Explorer or sign out and back in if necessary

### The menu opens an older build

- The registry is likely still pointing to an older executable path
- Run `uninstall`
- Reinstall using the latest release binary

### "Administrator privileges are required"

- Open PowerShell with `Run as Administrator`
- Retry the `install` or `uninstall` command

### No valid file path was found in the clipboard

- Run `Copy with mcopy` first
- Make sure the selected files or folders still exist
- Retry the paste command after refreshing the copy selection

## Closing Note

`mcopy` is built around a simple idea: keep the user-facing workflow familiar, but make the actual copy pipeline smarter, more observable, and easier to control. If you want a copy tool that feels native at the surface and structured under the hood, this codebase is built exactly for that.
