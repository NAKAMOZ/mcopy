// Hide the terminal window in Windows release builds.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use clap::{Parser, Subcommand};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::time::Instant;

mod clipboard;
mod context_menu;
mod ui;

// Reuse the shared copy pipeline from lib.rs.
use mcopy::{
    CopyController, ProgressPhase, ProgressUpdate, calculate_concurrency, collect_files,
    copy_files_with_progress, normalize_path, precreate_directories,
};

#[derive(Parser, Debug)]
#[command(author, version, about = "Asynchronous file and folder copy tool")]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Source folder (legacy mode)
    src: Option<PathBuf>,

    /// Target folder (legacy mode)
    dst: Option<PathBuf>,

    /// Number of concurrent copy tasks
    #[arg(long, short = 'j')]
    concurrency: Option<usize>,

    /// Disable the terminal progress bars
    #[arg(long)]
    no_progress: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Some(Commands::Install) => {
            // Admin check
            require_admin()?;

            // Resolve the executable path
            let exe = std::env::current_exe()?;
            println!("Exe path: {:?}", exe);

            // Install the context menu
            context_menu::install_context_menu(&exe)?;
        }

        Some(Commands::Uninstall) => {
            // Admin check
            require_admin()?;

            // Remove the context menu
            context_menu::uninstall_context_menu()?;
        }

        Some(Commands::Copy { paths, append }) => {
            // Copy the selected path(s) to the clipboard
            if append {
                clipboard::append_paths_to_clipboard(&paths)?;
            } else {
                clipboard::copy_paths_to_clipboard(&paths)?;
            }
            // Stay quiet when invoked from the context menu.
        }

        Some(Commands::Clear) => {
            clipboard::clear_clipboard()?;
        }

        Some(Commands::Paste { target }) => {
            // Normalize the path by stripping the Windows UNC prefix when needed.
            let target = normalize_path(target);

            // Create the target folder if it does not exist yet.
            if !target.exists() {
                std::fs::create_dir_all(&target)?;
            }

            // Read source paths from the clipboard.
            let sources = clipboard::paste_paths_from_clipboard()?;

            if sources.is_empty() {
                return Ok(()); // Exit quietly.
            }

            // Collect all files before opening the UI.
            let mut all_files = Vec::new();
            for src in &sources {
                let files = collect_files(src, &target).await?;
                all_files.extend(files);
            }

            if all_files.is_empty() {
                return Ok(());
            }

            // Build the shared progress state.
            let progress = ui::CopyProgress::new(all_files.len());
            let controller = CopyController::new();
            let progress_clone = progress.clone();
            let controller_clone = controller.clone();

            // Start the UI thread.
            let ui_thread = std::thread::spawn(move || {
                ui::show_progress_window(progress_clone, controller_clone);
            });

            // Pre-create destination folders.
            precreate_directories(&all_files).await?;

            if controller.is_cancelled() {
                progress.cancelled();
                let _ = ui_thread.join();
                return Ok(());
            }

            // Bridge copy updates into the UI state.
            let progress_for_callback = progress.clone();
            let callback = Box::new(move |update: ProgressUpdate| {
                progress_for_callback.apply(update);
            });

            // Start copying.
            let concurrency = calculate_concurrency(None);
            copy_files_with_progress(
                all_files,
                concurrency,
                Some(callback),
                Some(controller.clone()),
            )
            .await?;

            if controller.is_cancelled() {
                progress.cancelled();
            } else {
                progress.complete();
            }

            let _ = ui_thread.join();
        }

        None => {
            // Legacy CLI mode
            let src = args
                .src
                .ok_or_else(|| anyhow::anyhow!("Source folder is required"))?;
            let dst = args
                .dst
                .ok_or_else(|| anyhow::anyhow!("Target folder is required"))?;

            println!("Source: {:?}", src);
            println!("Target: {:?}", dst);

            let start = Instant::now();

            // Collect the files.
            let files = collect_files(&src, &dst).await?;
            println!("Total files: {}", files.len());

            // Pre-create destination folders.
            precreate_directories(&files).await?;

            // Resolve concurrency.
            let concurrency = calculate_concurrency(args.concurrency);
            println!("Concurrency: {}", concurrency);

            // Set up the legacy terminal progress bars.
            if !args.no_progress {
                let multi = MultiProgress::new();
                let overall = multi.add(ProgressBar::new(files.len() as u64));
                overall.set_style(
                    ProgressStyle::default_bar()
                        .template(
                            "[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} files ({percent}%)",
                        )
                        .unwrap()
                        .progress_chars("=>-"),
                );

                let current = multi.add(ProgressBar::new(0));
                current.set_style(
                    ProgressStyle::default_spinner()
                        .template("{spinner:.green} {msg}")
                        .unwrap(),
                );

                // Clone handles for the callback.
                let current_clone = current.clone();
                let overall_clone = overall.clone();

                // Feed progress updates into indicatif.
                let callback = Box::new(move |update: ProgressUpdate| match update.phase {
                    ProgressPhase::Started => {
                        current_clone.set_message(format!("Copying: {}", update.file_name));
                    }
                    ProgressPhase::Finished => {
                        current_clone.set_message(format!("Completed: {}", update.file_name));
                        overall_clone.set_position(update.processed_files as u64);
                    }
                    ProgressPhase::Failed => {
                        current_clone.set_message(format!("Skipped/Failed: {}", update.file_name));
                        overall_clone.set_position(update.processed_files as u64);
                    }
                });

                // Copy files.
                copy_files_with_progress(files, concurrency, Some(callback), None).await?;

                overall.finish_with_message("Copy completed!");
                current.finish_and_clear();
            } else {
                // Copy without terminal progress bars.
                copy_files_with_progress(files, concurrency, None, None).await?;
            }

            let elapsed = start.elapsed();
            println!("\nTotal time: {:.2?}", elapsed);
        }
    }

    Ok(())
}

/// Admin privilege check used only on Windows.
#[cfg(target_os = "windows")]
fn require_admin() -> anyhow::Result<()> {
    if !is_elevated::is_elevated() {
        anyhow::bail!(
            "Administrator privileges are required. Open PowerShell with 'Run as Administrator' and try again."
        );
    }
    Ok(())
}

/// Admin checks are not required on Unix-like systems.
#[cfg(not(target_os = "windows"))]
fn require_admin() -> anyhow::Result<()> {
    // On Unix we usually write into the current user's home directory.
    Ok(())
}
