use std::time::Duration;

pub const WINDOW_WIDTH: f32 = 560.0;
pub const WINDOW_HEIGHT: f32 = 200.0;
pub const AUTO_CLOSE_DELAY: Duration = Duration::from_millis(900);

pub const CARD_BG: u32 = 0xffffff;
pub const TITLE_TEXT: u32 = 0x111111;
pub const MUTED_TEXT: u32 = 0x999999;
pub const SOFT_TEXT: u32 = 0xb3b3b3;
pub const SUBTLE_BORDER: u32 = 0xe5e5e5;
pub const PROGRESS_TRACK: u32 = 0xebebeb;
pub const ACTIVE_FILL: u32 = 0x000000;
pub const PAUSED_FILL: u32 = 0xd4d4d4;
pub const SUCCESS_FILL: u32 = 0x22c55e;
pub const WARNING_FILL: u32 = 0xa3a3a3;
pub const DISABLED_BG: u32 = 0xfafafa;
pub const DISABLED_BORDER: u32 = 0xe5e5e5;
pub const DISABLED_TEXT: u32 = 0xb3b3b3;

#[derive(Clone, Copy)]
pub enum ButtonTone {
    Primary,
    Success,
    Outline,
}

impl ButtonTone {
    pub fn background(self) -> u32 {
        match self {
            Self::Primary => 0x000000,
            Self::Success => SUCCESS_FILL,
            Self::Outline => CARD_BG,
        }
    }

    pub fn hover_background(self) -> u32 {
        match self {
            Self::Primary => 0x1a1a1a,
            Self::Success => 0x16a34a,
            Self::Outline => 0xfafafa,
        }
    }

    pub fn active_background(self) -> u32 {
        match self {
            Self::Primary => 0x111111,
            Self::Success => 0x15803d,
            Self::Outline => 0xf5f5f5,
        }
    }

    pub fn border(self) -> u32 {
        match self {
            Self::Primary => 0x000000,
            Self::Success => SUCCESS_FILL,
            Self::Outline => SUBTLE_BORDER,
        }
    }

    pub fn text(self) -> u32 {
        match self {
            Self::Primary | Self::Success => CARD_BG,
            Self::Outline => MUTED_TEXT,
        }
    }
}
