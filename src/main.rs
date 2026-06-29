// Hide the terminal window in Windows builds.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod cli;

use clap::Parser;
use cli::commands;
use cli::{Args, Commands};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Some(Commands::Install) => commands::run_install()?,
        Some(Commands::Uninstall) => commands::run_uninstall()?,
        Some(Commands::Copy { paths, append }) => {
            commands::run_copy(&paths, append)?
        },
        Some(Commands::Clear) => commands::run_clear()?,
        Some(Commands::Paste { target }) => commands::run_paste(target).await?,
        None => commands::dispatch_default(args).await?,
    }

    Ok(())
}
