// Release build'de terminal penceresi gösterme (Windows)
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

// lib.rs'teki fonksiyonları kullan
use mcopy::{
    CopyController, ProgressPhase, ProgressUpdate, calculate_concurrency, collect_files,
    copy_files_with_progress, normalize_path, precreate_directories,
};

#[derive(Parser, Debug)]
#[command(author, version, about = "Asenkron klasör kopyalama aracı")]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Kaynak klasör (legacy mode)
    src: Option<PathBuf>,

    /// Hedef klasör (legacy mode)
    dst: Option<PathBuf>,

    /// Eşzamanlı işlem sayısı
    #[arg(long, short = 'j')]
    concurrency: Option<usize>,

    /// Progress bar'ı kapat
    #[arg(long)]
    no_progress: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Registry'ye context menu ekle (admin gerekli)
    Install,

    /// Registry'den context menu sil (admin gerekli)
    Uninstall,

    /// Clipboard'a path kopyala
    Copy {
        /// Kopyalanacak dosya/klasörler
        #[arg(required = true)]
        paths: Vec<PathBuf>,

        /// Mevcut clipboard'a ekle (çoklu seçim için)
        #[arg(long, short)]
        append: bool,
    },

    /// Clipboard'tan oku ve hedef klasöre kopyala
    Paste {
        /// Hedef klasör
        target: PathBuf,
    },

    /// Clipboard'ı temizle
    Clear,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Some(Commands::Install) => {
            // Admin kontrolü
            require_admin()?;

            // Exe path'i al
            let exe = std::env::current_exe()?;
            println!("Exe path: {:?}", exe);

            // Context menu kur
            context_menu::install_context_menu(&exe)?;
        }

        Some(Commands::Uninstall) => {
            // Admin kontrolü
            require_admin()?;

            // Context menu kaldır
            context_menu::uninstall_context_menu()?;
        }

        Some(Commands::Copy { paths, append }) => {
            // Clipboard'a path(ler) kopyala
            if append {
                clipboard::append_paths_to_clipboard(&paths)?;
            } else {
                clipboard::copy_paths_to_clipboard(&paths)?;
            }
            // Sessiz çalış - context menu'den çağrıldığında output yok
        }

        Some(Commands::Clear) => {
            clipboard::clear_clipboard()?;
        }

        Some(Commands::Paste { target }) => {
            // Path'i normalize et (Windows UNC prefix kaldır)
            let target = normalize_path(target);

            // Hedef klasör yoksa oluştur
            if !target.exists() {
                std::fs::create_dir_all(&target)?;
            }

            // Clipboard'tan paths oku
            let sources = clipboard::paste_paths_from_clipboard()?;

            if sources.is_empty() {
                return Ok(()); // Sessizce çık
            }

            // Önce tüm dosyaları topla
            let mut all_files = Vec::new();
            for src in &sources {
                let files = collect_files(src, &target).await?;
                all_files.extend(files);
            }

            if all_files.is_empty() {
                return Ok(());
            }

            // Progress durumunu oluştur
            let progress = ui::CopyProgress::new(all_files.len());
            let controller = CopyController::new();
            let progress_clone = progress.clone();
            let controller_clone = controller.clone();

            // UI thread'i başlat
            let ui_thread = std::thread::spawn(move || {
                ui::show_progress_window(progress_clone, controller_clone);
            });

            // Klasörleri oluştur
            precreate_directories(&all_files).await?;

            if controller.is_cancelled() {
                progress.cancelled();
                let _ = ui_thread.join();
                return Ok(());
            }

            // Progress callback oluştur
            let progress_for_callback = progress.clone();
            let callback = Box::new(move |update: ProgressUpdate| {
                progress_for_callback.apply(update);
            });

            // Kopyala
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
                .ok_or_else(|| anyhow::anyhow!("Kaynak klasör gerekli"))?;
            let dst = args
                .dst
                .ok_or_else(|| anyhow::anyhow!("Hedef klasör gerekli"))?;

            println!("Kaynak: {:?}", src);
            println!("Hedef:  {:?}", dst);

            let start = Instant::now();

            // Dosyaları topla
            let files = collect_files(&src, &dst).await?;
            println!("Toplam dosya: {}", files.len());

            // Klasörleri oluştur
            precreate_directories(&files).await?;

            // Concurrency hesapla
            let concurrency = calculate_concurrency(args.concurrency);
            println!("Eşzamanlılık: {}", concurrency);

            // Progress bar'ları hazırla (legacy indicatif)
            if !args.no_progress {
                let multi = MultiProgress::new();
                let overall = multi.add(ProgressBar::new(files.len() as u64));
                overall.set_style(
                    ProgressStyle::default_bar()
                        .template(
                            "[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} dosya ({percent}%)",
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

                // Clone for closure
                let current_clone = current.clone();
                let overall_clone = overall.clone();

                // Callback oluştur
                let callback = Box::new(move |update: ProgressUpdate| match update.phase {
                    ProgressPhase::Started => {
                        current_clone.set_message(format!("Kopyalanıyor: {}", update.file_name));
                    }
                    ProgressPhase::Finished => {
                        current_clone.set_message(format!("Tamamlandı: {}", update.file_name));
                        overall_clone.set_position(update.processed_files as u64);
                    }
                    ProgressPhase::Failed => {
                        current_clone.set_message(format!("Atlandı/Hata: {}", update.file_name));
                        overall_clone.set_position(update.processed_files as u64);
                    }
                });

                // Kopyala
                copy_files_with_progress(files, concurrency, Some(callback), None).await?;

                overall.finish_with_message("Kopyalama tamamlandı!");
                current.finish_and_clear();
            } else {
                // Progress yok
                copy_files_with_progress(files, concurrency, None, None).await?;
            }

            let elapsed = start.elapsed();
            println!("\nToplam süre: {:.2?}", elapsed);
        }
    }

    Ok(())
}

/// Admin yetkisi kontrolü (sadece Windows için gerekli)
#[cfg(target_os = "windows")]
fn require_admin() -> anyhow::Result<()> {
    if !is_elevated::is_elevated() {
        anyhow::bail!(
            "Admin yetkisi gerekli. PowerShell'i 'Run as Administrator' ile açın ve tekrar deneyin."
        );
    }
    Ok(())
}

/// Unix sistemlerde admin kontrolü (sudo ile çalıştırılması önerilir ama zorunlu değil)
#[cfg(not(target_os = "windows"))]
fn require_admin() -> anyhow::Result<()> {
    // Unix'te genelde sudo gerekli değil, kullanıcı home dizinine yazıyoruz
    Ok(())
}
