use std::time::Duration;

pub const WINDOW_WIDTH: f32 = 440.0;
pub const WINDOW_HEIGHT: f32 = 300.0;
pub const AUTO_CLOSE_DELAY: Duration = Duration::from_millis(900);

pub const PAGE_BG: u32 = 0x07111f;
pub const PANEL_BG: u32 = 0x0d1727;
pub const PANEL_BORDER: u32 = 0x1d2d47;
pub const CARD_BG: u32 = 0x111d2f;
pub const CARD_BORDER: u32 = 0x223352;
pub const FILE_CARD_BG: u32 = 0x101d31;
pub const TITLE_TEXT: u32 = 0xf8fafc;
pub const BODY_TEXT: u32 = 0xe2e8f0;
pub const MUTED_TEXT: u32 = 0x9fb3cc;
pub const LABEL_TEXT: u32 = 0x7f96b5;
pub const DISABLED_BG: u32 = 0x172435;
pub const DISABLED_BORDER: u32 = 0x2a3d56;
pub const DISABLED_TEXT: u32 = 0x6a809b;

#[derive(Clone, Copy)]
pub enum ButtonTone {
    Primary,
    Success,
    Danger,
}

impl ButtonTone {
    pub fn background(self) -> u32 {
        match self {
            Self::Primary => 0x2563eb,
            Self::Success => 0x059669,
            Self::Danger => 0xdc2626,
        }
    }

    pub fn hover_background(self) -> u32 {
        match self {
            Self::Primary => 0x1d4ed8,
            Self::Success => 0x047857,
            Self::Danger => 0xb91c1c,
        }
    }

    pub fn active_background(self) -> u32 {
        match self {
            Self::Primary => 0x1e40af,
            Self::Success => 0x065f46,
            Self::Danger => 0x991b1b,
        }
    }

    pub fn border(self) -> u32 {
        match self {
            Self::Primary => 0x3b82f6,
            Self::Success => 0x10b981,
            Self::Danger => 0xef4444,
        }
    }
}
