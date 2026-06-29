use gpui::{App, AssetSource, SharedString};
use std::borrow::Cow;

/// The bundled Inter faces, embedded at compile time.
///
/// `Inter` is hardcoded as the UI font but is not installed by default on
/// Windows or most Linux distros; without these, gpui silently falls back to a
/// system font and the absolute-pixel layouts shift. Register them once per
/// process before any window opens so `.font_family("Inter")` always resolves.
const INTER_FONTS: &[&[u8]] = &[
    include_bytes!("../../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../../assets/fonts/Inter-Bold.ttf"),
];

/// Register the bundled Inter faces with the text system.
///
/// Best-effort: a font that fails to load just falls back to the system font,
/// which is no worse than today's behavior, so we don't abort window startup.
pub(crate) fn register_fonts(cx: &App) {
    let fonts = INTER_FONTS.iter().map(|b| Cow::Borrowed(*b)).collect();
    if let Err(err) = cx.text_system().add_fonts(fonts) {
        eprintln!("warning: failed to load bundled Inter font: {err}");
    }
}

/// Single shared asset source serving the bundled `logo.svg` to every window.
///
/// Replaces the byte-identical `ProgressAssets` / `InstallAssets` that used to
/// live next to each window (OPTIMIZATIONS #5).
pub(crate) struct LogoAssets;

impl AssetSource for LogoAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(match path {
            "logo.svg" => Some(Cow::Borrowed(include_bytes!("../../logo.svg"))),
            _ => None,
        })
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(if path.is_empty() {
            vec![SharedString::from("logo.svg")]
        } else {
            Vec::new()
        })
    }
}
