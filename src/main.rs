// Hide the terminal window in Windows builds.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod cli;

use clap::Parser;
use cli::commands;
use cli::{Args, Commands};
use std::time::Duration;

/// How long to wait for background Tokio work after the command returns.
///
/// The copy engine is cooperative: cancelling lets in-flight `fs::copy` calls
/// finish rather than tearing a file in half. This bounds that wait so a stuck
/// filesystem cannot keep the process alive indefinitely.
const RUNTIME_SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// Build the Tokio runtime by hand rather than using `#[tokio::main]`.
///
/// GPUI's event loop must own the real main thread — a hard requirement on
/// macOS — and `Application::run` blocks until the last window closes. Under
/// `#[tokio::main]` that block happens *inside* a runtime worker, so runtime
/// teardown at process exit could stall behind the UI. Owning the runtime here
/// keeps the UI on the main thread and makes shutdown explicit and bounded.
fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let result = runtime.block_on(dispatch(args));

    // Drop the runtime with a deadline instead of letting `Drop` wait forever.
    runtime.shutdown_timeout(RUNTIME_SHUTDOWN_GRACE);

    result
}

async fn dispatch(args: Args) -> anyhow::Result<()> {
    match args.command {
        Some(Commands::ShellInstall) => commands::run_shell_install(),
        Some(Commands::ShellUninstall { all_users }) => {
            commands::run_shell_uninstall(all_users)
        },
        Some(Commands::Copy { paths, append }) => {
            commands::run_copy(&paths, append)
        },
        Some(Commands::Clear) => commands::run_clear(),
        Some(Commands::Status) => commands::run_status(),
        Some(Commands::Paste { target }) => commands::run_paste(target).await,
        None => commands::dispatch_default(args).await,
    }
}
