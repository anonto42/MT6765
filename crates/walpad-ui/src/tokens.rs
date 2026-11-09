//! Design tokens (docs/rust/UI.md). One dark theme, one accent. No gradients, no transparency,
//! no shadows: elevation is `SURFACE_RAISED` plus a 1 px border.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn hex(v: u32) -> Self {
        Color {
            r: (v >> 16) as u8,
            g: (v >> 8) as u8,
            b: v as u8,
        }
    }
    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
    pub(crate) fn sk(self) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.r, self.g, self.b, 255)
    }
}

pub const BG: Color = Color::hex(0x111316);
pub const SURFACE: Color = Color::hex(0x1A1D21);
pub const SURFACE_RAISED: Color = Color::hex(0x22262B);
pub const BORDER: Color = Color::hex(0x2C3137);
pub const TEXT: Color = Color::hex(0xE9ECEF);
pub const TEXT_SECONDARY: Color = Color::hex(0x9BA4AE);
pub const TEXT_DISABLED: Color = Color::hex(0x5D646C);
pub const ACCENT: Color = Color::hex(0x4FA89E);
pub const ACCENT_PRESSED: Color = Color::hex(0x3D8A81);
pub const ON_ACCENT: Color = Color::hex(0x0E1716);
pub const SUCCESS: Color = Color::hex(0x6DBE82);
pub const WARNING: Color = Color::hex(0xD9B25A);
pub const DANGER: Color = Color::hex(0xD9665E);

/// Spacing scale.
pub const SP: [i32; 7] = [4, 8, 12, 16, 24, 32, 48];
pub const SP_1: i32 = 4;
pub const SP_2: i32 = 8;
pub const SP_3: i32 = 12;
pub const SP_4: i32 = 16;
pub const SP_5: i32 = 24;
pub const SP_6: i32 = 32;
pub const SP_7: i32 = 48;

/// Corner radius: 12 for buttons and fields, 16 for cards and sheets.
pub const RADIUS: i32 = 12;
pub const RADIUS_CARD: i32 = 16;

/// Minimum touch target and fixed control heights.
pub const TOUCH_MIN: i32 = 56;
pub const ROW_H: i32 = 64;
pub const ROW_WIFI_H: i32 = 72;
pub const BUTTON_H: i32 = 56;
pub const ICON: i32 = 24;

/// Frame layout.
pub const STATUS_BAR_H: i32 = 40;
pub const NAV_H: i32 = 88;
pub const CONTENT_PAD: i32 = 24;
pub const DOCK_H: i32 = 56;

/// Type sizes in px (faces in `text::Face`).
pub const SIZE_CLOCK: f32 = 96.0;
pub const SIZE_TITLE: f32 = 28.0;
pub const SIZE_SECTION: f32 = 15.0;
pub const SIZE_BODY: f32 = 18.0;
pub const SIZE_SECONDARY: f32 = 15.0;
pub const SIZE_CAPTION: f32 = 13.0;
pub const SIZE_MONO: f32 = 15.0;

/// Interaction thresholds (ms / px).
pub const TAP_MAX_MS: u64 = 500;
pub const LONG_PRESS_MS: u64 = 500;
pub const SLOP_PX: i32 = 10;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_tokens_match_the_spec() {
        for (c, hex) in [
            (BG, "#111316"),
            (SURFACE, "#1A1D21"),
            (SURFACE_RAISED, "#22262B"),
            (BORDER, "#2C3137"),
            (TEXT, "#E9ECEF"),
            (TEXT_SECONDARY, "#9BA4AE"),
            (TEXT_DISABLED, "#5D646C"),
            (ACCENT, "#4FA89E"),
            (ACCENT_PRESSED, "#3D8A81"),
            (ON_ACCENT, "#0E1716"),
            (SUCCESS, "#6DBE82"),
            (WARNING, "#D9B25A"),
            (DANGER, "#D9665E"),
        ] {
            assert_eq!(c.to_hex(), hex);
        }
    }

    #[test]
    fn spacing_scale_and_minimums() {
        assert_eq!(SP, [4, 8, 12, 16, 24, 32, 48]);
        assert!(ROW_H >= TOUCH_MIN && BUTTON_H >= TOUCH_MIN && ROW_WIFI_H >= TOUCH_MIN);
        // portrait: status bar + content + nav = 1280
        assert_eq!(STATUS_BAR_H + (1280 - STATUS_BAR_H - NAV_H) + NAV_H, 1280);
        assert_eq!(1280 - NAV_H, 1192, "content ends at y 1192 as in the spec");
    }
}
