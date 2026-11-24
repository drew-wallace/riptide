use gpui::{Rgba, rgb, rgba};

/// TIDAL-inspired dark audiophile design tokens.
/// Deep obsidian blacks, subtle cool-charcoal elevation, razor-sharp hairlines,
/// crisp pure-white primary typography, and electric cyan (`#00FFFF`) accents.
pub struct TidalTheme;

impl TidalTheme {
    // Core surfaces
    pub const BG_VOID: u32 = 0x000000;
    pub const BG_SIDEBAR: u32 = 0x060608;
    pub const BG_MAIN: u32 = 0x0B0B0E;
    pub const BG_SURFACE: u32 = 0x131318;
    pub const BG_SURFACE_HOVER: u32 = 0x1C1C23;
    pub const BG_SURFACE_ACTIVE: u32 = 0x24242E;
    pub const BG_PLAYER: u32 = 0x101014;
    pub const BG_DRAWER: u32 = 0x0D0D11;
    pub const BG_INPUT: u32 = 0x18181F;

    // Borders & hairlines
    pub const BORDER_SUBTLE: u32 = 0x1E1E26;
    pub const BORDER_DEFAULT: u32 = 0x2A2A35;
    pub const BORDER_STRONG: u32 = 0x3D3D4D;

    // Typography
    pub const TEXT_PRIMARY: u32 = 0xFAFAFC;
    pub const TEXT_SECONDARY: u32 = 0xA0A0AE;
    pub const TEXT_MUTED: u32 = 0x676775;
    pub const TEXT_INVERSE: u32 = 0x050507;

    // Signature TIDAL accents
    pub const ACCENT_CYAN: u32 = 0x00FFFF;
    pub const ACCENT_CYAN_SOFT: u32 = 0x29E3E3;
    pub const ACCENT_GOLD: u32 = 0xFFD43B;
    pub const STATUS_ONLINE: u32 = 0x22C55E;
    pub const STATUS_WARN: u32 = 0xF59E0B;
    pub const STATUS_DANGER: u32 = 0xEF4444;

    pub fn bg_void() -> Rgba {
        rgb(Self::BG_VOID)
    }
    pub fn bg_sidebar() -> Rgba {
        rgb(Self::BG_SIDEBAR)
    }
    pub fn bg_main() -> Rgba {
        rgb(Self::BG_MAIN)
    }
    pub fn bg_surface() -> Rgba {
        rgb(Self::BG_SURFACE)
    }
    pub fn bg_surface_hover() -> Rgba {
        rgb(Self::BG_SURFACE_HOVER)
    }
    pub fn bg_surface_active() -> Rgba {
        rgb(Self::BG_SURFACE_ACTIVE)
    }
    pub fn bg_player() -> Rgba {
        rgb(Self::BG_PLAYER)
    }
    pub fn bg_drawer() -> Rgba {
        rgb(Self::BG_DRAWER)
    }
    pub fn bg_input() -> Rgba {
        rgb(Self::BG_INPUT)
    }

    pub fn border_subtle() -> Rgba {
        rgb(Self::BORDER_SUBTLE)
    }
    pub fn border_default() -> Rgba {
        rgb(Self::BORDER_DEFAULT)
    }
    pub fn border_strong() -> Rgba {
        rgb(Self::BORDER_STRONG)
    }

    pub fn text_primary() -> Rgba {
        rgb(Self::TEXT_PRIMARY)
    }
    pub fn text_secondary() -> Rgba {
        rgb(Self::TEXT_SECONDARY)
    }
    pub fn text_muted() -> Rgba {
        rgb(Self::TEXT_MUTED)
    }
    pub fn text_inverse() -> Rgba {
        rgb(Self::TEXT_INVERSE)
    }

    pub fn accent() -> Rgba {
        rgb(Self::ACCENT_CYAN)
    }
    pub fn accent_soft() -> Rgba {
        rgb(Self::ACCENT_CYAN_SOFT)
    }
    pub fn accent_tint() -> Rgba {
        rgba(0x00FFFF18)
    }
    pub fn accent_tint_strong() -> Rgba {
        rgba(0x00FFFF30)
    }
    pub fn gold() -> Rgba {
        rgb(Self::ACCENT_GOLD)
    }
    pub fn gold_tint() -> Rgba {
        rgba(0xFFD43B1F)
    }
    pub fn online() -> Rgba {
        rgb(Self::STATUS_ONLINE)
    }
    pub fn warn() -> Rgba {
        rgb(Self::STATUS_WARN)
    }
    pub fn danger() -> Rgba {
        rgb(Self::STATUS_DANGER)
    }

    pub fn overlay_dark() -> Rgba {
        rgba(0x000000B0)
    }
}
