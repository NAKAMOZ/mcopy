use crate::cli::Args;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use mcopy::clipboard;
use mcopy::ui;
use mcopy::platform::{self, ContextMenu, Platform};
use mcopy::{
    CopyController, ProgressPhase, ProgressUpdate, calculate_concurrency, collect_files,
    copy_files_with_progress, normalize_path, precreate_directories,
};
use std::path::PathBuf;
use std::time::Instant;

/// `mcopy install` — install or replace the context-menu integration.
pub fn run_install() -> anyhow::Result<()> {
    require_admin()?;

    let exe = std::env::current_exe()?;
    println!("Exe path: {:?}", exe);

    platform::install_or_update_context_menu(&exe)
}

/// `mcopy uninstall` — remove the context-menu integration.
pub fn run_uninstall() -> anyhow::Result<()> {
    require_admin()?;
    Platform::uninstall()
}

/// `mcopy copy <paths…>` — write the selection into the clipboard.
pub fn run_copy(paths: &[PathBuf], append: bool) -> anyhow::Result<()> {
    if append {
        clipboard::append_paths_to_clipboard(paths)?;
    } else {
        clipboard::copy_paths_to_clipboard(paths)?;
    }
    // Stay quiet when invoked from the context menu.
    Ok(())
}

/// `mcopy clear` — empty the clipboard payload.
pub fn run_clear() -> anyhow::Result<()> {
    clipboard::clear_clipboard()
}

/// `mcopy paste <target>` — copy the clipboard paths into `target`, driving the
/// GPUI progress window.
pub async fn run_paste(target: PathBuf) -> anyhow::Result<()> {
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
    Ok(())
}

/// No subcommand: open the install window when no paths were given, otherwise
/// run the legacy `mcopy <src> <dst>` terminal copy.
pub async fn dispatch_default(args: Args) -> anyhow::Result<()> {
    if args.src.is_none() && args.dst.is_none() {
        let exe = std::env::current_exe()?;
        ui::show_install_window(exe);
        return Ok(());
    }

    run_legacy(args).await
}

/// Legacy CLI copy with `indicatif` terminal progress bars.
async fn run_legacy(args: Args) -> anyhow::Result<()> {
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
                .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} files ({percent}%)")
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
