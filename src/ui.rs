use gpui::*;
use gpui_component::progress::Progress;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};

/// Kopyalama ilerleme durumu (thread-safe)
#[derive(Clone)]
pub struct CopyProgress {
    pub current_file: Arc<Mutex<String>>,
    pub files_copied: Arc<AtomicUsize>,
    pub total_files: Arc<AtomicUsize>,
    pub bytes_copied: Arc<AtomicU64>,
    pub total_bytes: Arc<AtomicU64>,
    pub is_complete: Arc<AtomicBool>,
}

impl CopyProgress {
    pub fn new(total_files: usize) -> Self {
        Self {
            current_file: Arc::new(Mutex::new(String::new())),
            files_copied: Arc::new(AtomicUsize::new(0)),
            total_files: Arc::new(AtomicUsize::new(total_files)),
            bytes_copied: Arc::new(AtomicU64::new(0)),
            total_bytes: Arc::new(AtomicU64::new(0)),
            is_complete: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn update(&self, filename: String, files_done: usize, bytes: u64, total_bytes: u64) {
        *self.current_file.lock().unwrap() = filename;
        self.files_copied.store(files_done, Ordering::SeqCst);
        self.bytes_copied.store(bytes, Ordering::SeqCst);
        self.total_bytes.store(total_bytes, Ordering::SeqCst);
    }

    pub fn complete(&self) {
        self.is_complete.store(true, Ordering::SeqCst);
    }

    pub fn get_percent(&self) -> f32 {
        let total = self.total_files.load(Ordering::SeqCst);
        if total == 0 {
            return 0.0;
        }
        let copied = self.files_copied.load(Ordering::SeqCst);
        (copied as f32 / total as f32) * 100.0
    }
}

/// GPUI Progress penceresi
pub struct ProgressWindow {
    progress: CopyProgress,
}

impl ProgressWindow {
    pub fn new(progress: CopyProgress) -> Self {
        Self { progress }
    }
}

impl Render for ProgressWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let files_copied = self.progress.files_copied.load(Ordering::SeqCst);
        let total_files = self.progress.total_files.load(Ordering::SeqCst);
        let current_file = self.progress.current_file.lock().unwrap().clone();
        let percent = self.progress.get_percent();
        let is_complete = self.progress.is_complete.load(Ordering::SeqCst);

        // Tamamlandıysa pencereyi kapat
        if is_complete {
            cx.quit();
        }

        // Her 100ms'de bir yeniden çiz - notify ile güncelleme
        if !is_complete {
            cx.notify();
        }

        let status_text = if is_complete {
            "Tamamlandı!".to_string()
        } else {
            format!("Kopyalanıyor: {}/{} dosya", files_copied, total_files)
        };

        let file_display = if current_file.len() > 50 {
            format!("...{}", &current_file[current_file.len() - 47..])
        } else {
            current_file
        };

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .size_full()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xffffff))
            .child(
                div()
                    .text_base()
                    .font_weight(FontWeight::BOLD)
                    .child(status_text),
            )
            .child(Progress::new().value(percent).w_full())
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x888888))
                            .child(file_display),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x888888))
                            .child(format!("{:.0}%", percent)),
                    ),
            )
    }
}

/// Progress penceresi göster
pub fn show_progress_window(progress: CopyProgress) {
    Application::new().run(|cx| {
        gpui_component::init(cx);

        let bounds = Bounds::centered(None, size(px(400.), px(120.)), cx);

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("mcopy - Kopyalanıyor...".into()),
                appears_transparent: false,
                ..Default::default()
            }),
            focus: true,
            show: true,
            kind: WindowKind::PopUp,
            ..Default::default()
        };

        cx.open_window(options, |_, cx| cx.new(|_| ProgressWindow::new(progress)))
            .unwrap();
    });
}
