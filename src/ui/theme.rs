//! Colors and metrics for both windows, resolved per OS appearance.
//!
//! Everything the UI paints comes from a [`Palette`] chosen from the window's
//! current [`WindowAppearance`], so light and dark are two data values rather
//! than two code paths. The one exception is [`LOGO_ACCENT`], which is a free
//! constant precisely so no palette can reach it.

use gpui::WindowAppearance;
use std::time::Duration;

pub const WINDOW_WIDTH: f32 = 560.0;
pub const WINDOW_HEIGHT: f32 = 200.0;
pub const ACTION_BUTTON_WIDTH: f32 = 92.0;
pub const AUTO_CLOSE_DELAY: Duration = Duration::from_millis(900);

/// The green segment of the mcopy mark.
///
/// Intentionally **not** a [`Palette`] field: the brand green must render
/// identically in every appearance, and keeping it outside the palette makes
/// that a structural guarantee rather than a convention someone can break by
/// adding a dark-mode override. The logo is drawn as five independently
/// colored rectangles, so there is also no filter or blend step that could
/// shift this hue as a side effect of theming the rest of the surface.
pub const LOGO_ACCENT: u32 = 0x22c55e;

/// Whether a resolved palette is the light or the dark variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
}

/// Every themed color used by the two windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub mode: Mode,

    /// Ink for the four dark bars of the logo: black in light, white in dark.
    pub logo_ink: u32,

    pub card_bg: u32,
    pub title_text: u32,
    pub muted_text: u32,
    pub soft_text: u32,
    pub subtle_border: u32,
    pub progress_track: u32,

    pub active_fill: u32,
    pub paused_fill: u32,
    pub success_fill: u32,
    pub warning_fill: u32,

    pub disabled_bg: u32,
    pub disabled_border: u32,
    pub disabled_text: u32,

    /// Neutral (high-contrast) button fill and its hover/active shades.
    pub neutral_fill: u32,
    pub neutral_hover: u32,
    pub neutral_active: u32,
    /// Text drawn on top of `neutral_fill` / `success_fill`.
    pub on_fill_text: u32,

    pub success_hover: u32,
    pub success_active: u32,

    pub outline_hover_bg: u32,
    pub outline_active_bg: u32,

    pub error_text: u32,
    pub install_disabled_bg: u32,
}

impl Palette {
    pub const fn light() -> Self {
        Self {
            mode: Mode::Light,
            logo_ink: 0x000000,

            card_bg: 0xffffff,
            title_text: 0x111111,
            muted_text: 0x999999,
            soft_text: 0xb3b3b3,
            subtle_border: 0xe5e5e5,
            progress_track: 0xebebeb,

            active_fill: 0x000000,
            paused_fill: 0xd4d4d4,
            success_fill: LOGO_ACCENT,
            warning_fill: 0xa3a3a3,

            disabled_bg: 0xfafafa,
            disabled_border: 0xe5e5e5,
            disabled_text: 0xb3b3b3,

            neutral_fill: 0x000000,
            neutral_hover: 0x1a1a1a,
            neutral_active: 0x111111,
            on_fill_text: 0xffffff,

            success_hover: 0x20b956,
            success_active: 0x15803d,

            outline_hover_bg: 0xfafafa,
            outline_active_bg: 0xf5f5f5,

            error_text: 0x8a8a8a,
            install_disabled_bg: 0xe5e5e5,
        }
    }

    /// Dark variant.
    ///
    /// The neutral scale is inverted rather than merely darkened, so the
    /// "black" surfaces of light mode become light surfaces here and the logo
    /// ink flips to white. `success_fill` stays [`LOGO_ACCENT`] in both.
    pub const fn dark() -> Self {
        Self {
            mode: Mode::Dark,
            logo_ink: 0xffffff,

            card_bg: 0x1c1c1e,
            title_text: 0xf5f5f5,
            muted_text: 0x8e8e93,
            soft_text: 0x6c6c70,
            subtle_border: 0x3a3a3c,
            progress_track: 0x3a3a3c,

            active_fill: 0xf5f5f5,
            paused_fill: 0x5a5a5e,
            success_fill: LOGO_ACCENT,
            warning_fill: 0x7c7c80,

            disabled_bg: 0x2c2c2e,
            disabled_border: 0x3a3a3c,
            disabled_text: 0x6c6c70,

            neutral_fill: 0xf5f5f5,
            neutral_hover: 0xe0e0e0,
            neutral_active: 0xd0d0d0,
            on_fill_text: 0x1c1c1e,

            success_hover: 0x20b956,
            success_active: 0x15803d,

            outline_hover_bg: 0x2c2c2e,
            outline_active_bg: 0x3a3a3c,

            error_text: 0xff6b6b,
            install_disabled_bg: 0x3a3a3c,
        }
    }

    /// Pick the palette matching the OS appearance reported for a window.
    ///
    /// The vibrant variants are macOS-specific tints of the same two modes, so
    /// they map onto the same palettes.
    pub const fn for_appearance(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Light | WindowAppearance::VibrantLight => {
                Self::light()
            },
            WindowAppearance::Dark | WindowAppearance::VibrantDark => {
                Self::dark()
            },
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::light()
    }
}

/// Semantic role of a button, resolved against a [`Palette`] at paint time.
#[derive(Clone, Copy)]
pub enum ButtonTone {
    /// High-contrast neutral fill (the default action).
    Primary,
    /// Brand-green fill (a positive, confirming action).
    Success,
    /// Bordered, transparent fill (a secondary action).
    Outline,
}

impl ButtonTone {
    pub fn background(self, palette: &Palette) -> u32 {
        match self {
            Self::Primary => palette.neutral_fill,
            Self::Success => palette.success_fill,
            Self::Outline => palette.card_bg,
        }
    }

    pub fn hover_background(self, palette: &Palette) -> u32 {
        match self {
            Self::Primary => palette.neutral_hover,
            Self::Success => palette.success_hover,
            Self::Outline => palette.outline_hover_bg,
        }
    }

    pub fn active_background(self, palette: &Palette) -> u32 {
        match self {
            Self::Primary => palette.neutral_active,
            Self::Success => palette.success_active,
            Self::Outline => palette.outline_active_bg,
        }
    }

    pub fn border(self, palette: &Palette) -> u32 {
        match self {
            Self::Primary => palette.neutral_fill,
            Self::Success => palette.success_fill,
            Self::Outline => palette.subtle_border,
        }
    }

    pub fn text(self, palette: &Palette) -> u32 {
        match self {
            Self::Primary | Self::Success => palette.on_fill_text,
            Self::Outline => palette.muted_text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_APPEARANCES: [WindowAppearance; 4] = [
        WindowAppearance::Light,
        WindowAppearance::VibrantLight,
        WindowAppearance::Dark,
        WindowAppearance::VibrantDark,
    ];

    #[test]
    fn appearance_maps_to_the_expected_mode() {
        assert_eq!(
            Palette::for_appearance(WindowAppearance::Light).mode,
            Mode::Light
        );
        assert_eq!(
            Palette::for_appearance(WindowAppearance::VibrantLight).mode,
            Mode::Light
        );
        assert_eq!(
            Palette::for_appearance(WindowAppearance::Dark).mode,
            Mode::Dark
        );
        assert_eq!(
            Palette::for_appearance(WindowAppearance::VibrantDark).mode,
            Mode::Dark
        );
    }

    #[test]
    fn logo_ink_is_black_in_light_and_white_in_dark() {
        assert_eq!(Palette::light().logo_ink, 0x000000);
        assert_eq!(Palette::dark().logo_ink, 0xffffff);
    }

    /// The headline guarantee of issue 1: the green section never changes.
    #[test]
    fn logo_accent_is_identical_in_every_appearance() {
        for appearance in ALL_APPEARANCES {
            let palette = Palette::for_appearance(appearance);
            assert_eq!(
                palette.success_fill, LOGO_ACCENT,
                "success fill drifted from the brand green in {appearance:?}"
            );
        }

        assert_eq!(Palette::light().success_fill, Palette::dark().success_fill);
        assert_eq!(LOGO_ACCENT, 0x22c55e);
    }

    /// The logo ink must contrast with the surface it is drawn on; a white mark
    /// on a white card was the failure mode this palette exists to prevent.
    #[test]
    fn logo_ink_contrasts_with_the_card_background() {
        for appearance in ALL_APPEARANCES {
            let palette = Palette::for_appearance(appearance);
            assert!(
                relative_luminance_gap(palette.logo_ink, palette.card_bg) > 0.5,
                "logo ink does not contrast with the card in {appearance:?}"
            );
        }
    }

    #[test]
    fn text_contrasts_with_the_card_background() {
        for appearance in ALL_APPEARANCES {
            let palette = Palette::for_appearance(appearance);
            assert!(
                relative_luminance_gap(palette.title_text, palette.card_bg)
                    > 0.5,
                "title text does not contrast with the card in {appearance:?}"
            );
        }
    }

    #[test]
    fn button_text_contrasts_with_its_fill() {
        for appearance in ALL_APPEARANCES {
            let palette = Palette::for_appearance(appearance);
            for tone in [ButtonTone::Primary, ButtonTone::Success] {
                assert!(
                    relative_luminance_gap(
                        tone.text(&palette),
                        tone.background(&palette)
                    ) > 0.3,
                    "button label is illegible on its fill in {appearance:?}"
                );
            }
        }
    }

    /// Perceptual luminance difference between two packed 0xRRGGBB colors,
    /// in the range 0.0 (identical) to 1.0 (black vs. white).
    fn relative_luminance_gap(a: u32, b: u32) -> f32 {
        (luminance(a) - luminance(b)).abs()
    }

    fn luminance(color: u32) -> f32 {
        let r = ((color >> 16) & 0xff) as f32 / 255.0;
        let g = ((color >> 8) & 0xff) as f32 / 255.0;
        let b = (color & 0xff) as f32 / 255.0;
        // Rec. 601 luma weights: adequate for a light/dark sanity check.
        0.299 * r + 0.587 * g + 0.114 * b
    }
}
