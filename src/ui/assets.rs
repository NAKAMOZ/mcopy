use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

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
