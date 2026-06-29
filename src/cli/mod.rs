pub mod commands;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about = "Asynchronous file and folder copy tool")]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Source folder (legacy mode)
    pub src: Option<PathBuf>,

    /// Target folder (legacy mode)
    pub dst: Option<PathBuf>,

    /// Number of concurrent copy tasks
    #[arg(long, short = 'j')]
    pub concurrency: Option<usize>,

    /// Disable the terminal progress bars
    #[arg(long)]
    pub no_progress: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Install the context menu into the registry (requires admin)
    Install,

    /// Remove the context menu from the registry (requires admin)
    Uninstall,

    /// Copy paths into the clipboard
    Copy {
        /// Files or folders to copy
        #[arg(required = true)]
        paths: Vec<PathBuf>,

        /// Append to the existing clipboard payload
        #[arg(long, short)]
        append: bool,
    },

    /// Read clipboard paths and copy them into the target folder
    Paste {
        /// Target folder
        target: PathBuf,
    },

    /// Clear the clipboard payload
    Clear,
}
