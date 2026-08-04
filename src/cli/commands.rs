use crate::cli::Args;
use futures::{StreamExt, stream};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use mcopy::clipboard::{self, PasteRefusal};
use mcopy::platform::{self, ContextMenu, Platform};
use mcopy::ui;
use mcopy::{
    CopyController, CopyItem, ProgressPhase, ProgressUpdate,
    calculate_concurrency, collect_files, copy_files_with_progress, errln,
    log_error, log_info, normalize_path, outln, precreate_directories,
    repair_shell_argument,
};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// `mcopy shell-install` — register the file manager integration.
///
/// No elevation: 0.3 registers per-user entries only. See
/// [`mcopy::platform`] for why.
pub fn run_shell_install() -> anyhow::Result<()> {
    let location = platform::location::detect()?;
    if let Some(reason) = location.blocking_reason() {
        anyhow::bail!(
            "mcopy is running from a location that will not persist. {}",
            reason.remedy()
        );
    }

    platform::install_or_update_context_menu(location.exe())?;
    // Reflect whatever is already copied, so a fresh install does not show a
    // Paste entry that would do nothing.
    clipboard::resync_paste_visibility();

    outln!("Registered the mcopy file manager integration.");
    Ok(())
}

/// `mcopy shell-uninstall` — remove the file manager integration.
pub fn run_shell_uninstall(all_users: bool) -> anyhow::Result<()> {
    Platform::uninstall()?;

    if all_users {
        remove_machine_wide_entries()?;
    }

    outln!("Removed the mcopy file manager integration.");
    Ok(())
}

#[cfg(target_os = "windows")]
fn remove_machine_wide_entries() -> anyhow::Result<()> {
    if !is_elevated::is_elevated() {
        anyhow::bail!(
            "Removing the machine-wide entries left by mcopy 0.2 requires \
             administrator rights. Open a terminal with \"Run as \
             administrator\" and run: mcopy shell-uninstall --all-users"
        );
    }

    platform::uninstall_all_users()?;
    outln!("Removed the machine-wide entries left by mcopy 0.2.");
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn remove_machine_wide_entries() -> anyhow::Result<()> {
    // Only Windows 0.2 ever wrote outside the user's own directories.
    Ok(())
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

/// `mcopy status` — report what is currently copied.
pub fn run_status() -> anyhow::Result<()> {
    let state = clipboard::current_state();
    if state.is_empty() {
        outln!("Nothing is copied.");
    } else {
        outln!("{} item(s) ready to paste:", state.items().len());
        for item in state.items() {
            outln!("  {}", item.display());
        }
    }
    Ok(())
}

/// `mcopy paste <target>` — copy the clipboard paths into `target`, driving the
/// GPUI progress window.
pub async fn run_paste(target: PathBuf) -> anyhow::Result<()> {
    // Undo Explorer's command-line quoting artifact before anything touches the
    // path, then strip the Windows UNC prefix when present.
    let target = normalize_path(repair_shell_argument(target));

    // Take the single-flight lock and read the validated sources together, so
    // two quick right-click Pastes cannot both proceed. The guard is held for
    // the whole operation and released even if this returns early.
    let (sources, session, _lock) = match clipboard::begin_paste() {
        Ok(started) => started,
        Err(refusal) => {
            log_info!("paste refused: {refusal:?}");
            return report_refusal(refusal);
        },
    };

    if let Err(error) = validate_destination(&target, &sources) {
        return report_paste_error(&error.to_string());
    }

    // Collect all filesystem items before opening the UI. Walk independent
    // source roots concurrently (bounded) so pasting many folders overlaps
    // their traversal.
    let concurrency = calculate_concurrency(None);
    let per_source: Vec<anyhow::Result<Vec<CopyItem>>> = stream::iter(&sources)
        .map(|src| collect_files(src, &target))
        .buffer_unordered(concurrency)
        .collect()
        .await;

    let mut all_files = Vec::new();
    for files in per_source {
        match files {
            Ok(files) => all_files.extend(files),
            Err(error) => {
                log_error!("could not read a copied source: {error}");
                return report_paste_error(&format!(
                    "Could not read a copied item: {error}"
                ));
            },
        }
    }

    if all_files.is_empty() {
        return report_refusal(PasteRefusal::NothingToPaste);
    }

    log_info!(
        "pasting {} item(s) from {} source(s) into {}",
        all_files.len(),
        sources.len(),
        target.display()
    );

    // Build the shared progress state.
    let progress = ui::CopyProgress::new(all_files.len());
    let controller = CopyController::new();
    let progress_clone = progress.clone();
    let controller_clone = controller.clone();

    // GPUI must be constructed on the main thread on macOS. Keep the UI on the
    // current thread and run the copy job on Tokio worker threads.
    let copy_task = tokio::spawn(async move {
        let result = async {
            // Pre-create destination folders.
            precreate_directories(&all_files).await?;

            if controller_clone.is_cancelled() {
                return Ok(());
            }

            // Bridge copy updates into the UI state.
            let progress_for_callback = progress_clone.clone();
            let callback = Box::new(move |update: ProgressUpdate| {
                progress_for_callback.apply(update);
            });

            // Start copying.
            let concurrency = calculate_concurrency(None);
            copy_files_with_progress(
                all_files,
                concurrency,
                Some(callback),
                Some(controller_clone.clone()),
            )
            .await
        }
        .await;

        if controller_clone.is_cancelled() {
            progress_clone.cancelled();
        } else if result.is_ok() {
            progress_clone.complete();
        } else {
            progress_clone.cancelled();
        }

        result
    });

    ui::show_progress_window(progress.clone(), controller.clone());

    let outcome = copy_task.await?;

    // Consume the copy state only when the paste actually finished: a
    // cancelled or failed run keeps it so the user can simply try again.
    let snapshot_failures = progress.failed_count();
    if outcome.is_ok() && !controller.is_cancelled() && snapshot_failures == 0 {
        clipboard::finish_paste(session);
    } else {
        log_info!(
            "keeping the copy state (cancelled={}, failures={})",
            controller.is_cancelled(),
            snapshot_failures
        );
    }

    outcome
}

/// Reject a destination before any work starts.
///
/// 0.2 called `create_dir_all` and began copying, so an unwritable or nonsense
/// destination only surfaced as a wall of per-item failures with no cause.
fn validate_destination(
    target: &Path,
    sources: &[PathBuf],
) -> anyhow::Result<()> {
    if target.exists() {
        if !target.is_dir() {
            anyhow::bail!(
                "The paste destination is not a folder: {}",
                target.display()
            );
        }
    } else {
        std::fs::create_dir_all(target).map_err(|error| {
            anyhow::anyhow!(
                "Could not create the destination folder {}: {}",
                target.display(),
                describe_io_error(&error)
            )
        })?;
    }

    // Probe for write access rather than inspecting the mode: permissions,
    // ACLs, read-only mounts and sandbox policy all differ, and only an actual
    // write answers the question on every platform.
    let probe =
        target.join(format!(".mcopy-write-test-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
        },
        Err(error) => {
            anyhow::bail!(
                "Cannot write to {}: {}",
                target.display(),
                describe_io_error(&error)
            );
        },
    }

    if clipboard::target_is_inside_sources(target, sources) {
        anyhow::bail!(
            "Cannot paste a folder into itself: {}",
            target.display()
        );
    }

    Ok(())
}

/// Turn an I/O error into a sentence with a next step where one exists.
fn describe_io_error(error: &std::io::Error) -> String {
    let kind = mcopy::CopyErrorKind::classify(error);
    match kind.hint() {
        Some(hint) => format!("{} ({}). {hint}", error, kind.describe()),
        None => error.to_string(),
    }
}

/// Surface a refusal the same way on every platform.
///
/// The GUI paths are launched from a file manager with no console attached, so
/// a message the user can actually see has to be a window.
fn report_refusal(refusal: PasteRefusal) -> anyhow::Result<()> {
    report_paste_error(refusal.message())
}

fn report_paste_error(message: &str) -> anyhow::Result<()> {
    errln!("{message}");
    ui::show_notice_window("mcopy", message);
    Ok(())
}

/// No subcommand: open the setup window when no paths were given, otherwise
/// run the legacy `mcopy <src> <dst>` terminal copy.
pub async fn dispatch_default(args: Args) -> anyhow::Result<()> {
    if args.src.is_none() && args.dst.is_none() {
        let exe = std::env::current_exe()?;
        // Correct any stale menu visibility left by a crash or a reboot before
        // the user has a chance to see it.
        clipboard::resync_paste_visibility();
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

    outln!("Source: {:?}", src);
    outln!("Target: {:?}", dst);

    let start = Instant::now();

    // Collect files, directories, and symlinks.
    let files = collect_files(&src, &dst).await?;
    outln!("Total items: {}", files.len());

    // Pre-create destination folders.
    precreate_directories(&files).await?;

    // Resolve concurrency.
    let concurrency = calculate_concurrency(args.concurrency);
    outln!("Concurrency: {}", concurrency);

    // Set up the legacy terminal progress bars.
    if !args.no_progress {
        let multi = MultiProgress::new();
        let overall = multi.add(ProgressBar::new(files.len() as u64));
        overall.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} items ({percent}%)")
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
        let callback =
            Box::new(move |update: ProgressUpdate| match update.phase {
                ProgressPhase::Started => {
                    current_clone
                        .set_message(format!("Copying: {}", update.file_name));
                },
                ProgressPhase::Finished => {
                    current_clone.set_message(format!(
                        "Completed: {}",
                        update.file_name
                    ));
                    overall_clone.set_position(update.processed_files as u64);
                },
                ProgressPhase::Failed => {
                    let reason = update
                        .error
                        .map(|kind| kind.describe())
                        .unwrap_or("unknown error");
                    current_clone.set_message(format!(
                        "Skipped ({reason}): {}",
                        update.file_name
                    ));
                    overall_clone.set_position(update.processed_files as u64);
                },
            });

        // Copy files.
        copy_files_with_progress(files, concurrency, Some(callback), None)
            .await?;

        overall.finish_with_message("Copy completed!");
        current.finish_and_clear();
    } else {
        // Copy without terminal progress bars.
        copy_files_with_progress(files, concurrency, None, None).await?;
    }

    let elapsed = start.elapsed();
    outln!("\nTotal time: {:.2?}", elapsed);
    Ok(())
}
