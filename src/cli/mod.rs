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
    /// Register the file manager integration for the current user
    ///
    /// `install` is kept as an alias so scripts written against 0.2 keep
    /// working, even though the name is now a little misleading: application
    /// installation is the installer's job, and this only registers menus.
    #[command(alias = "install")]
    ShellInstall,

    /// Remove the file manager integration
    #[command(alias = "uninstall")]
    ShellUninstall {
        /// Also remove the machine-wide entries left by mcopy 0.2
        ///
        /// This is the only operation that needs administrator rights, and it
        /// exists solely to clean up after the previous version.
        #[arg(long)]
        all_users: bool,
    },

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

    /// Print what is currently copied
    Status,
}
