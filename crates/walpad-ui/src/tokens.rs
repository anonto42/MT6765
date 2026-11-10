//! Design tokens (docs/rust/UI.md): the line-art theme. One charcoal background, white strokes and
//! text. "White at N %" stroke/text colours are pre-blended over the background at compile time
//! (opaque writes). Surfaces (popovers, cards, rows, buttons, icon boxes, sheets, the dock, the
//! keyboard, the mouse pad) get a translucent dark fill (`Tint`, owner request 2026-11-10) that is
//! alpha-blended over whatever is beneath (the wallpaper art), so a component always reads apart
//! from the background. No gradients, no shadows.

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
    /// `top` at `pct` % opacity over `self`, as one opaque colour (rounded).
    pub const fn mix(self, top: Color, pct: u32) -> Color {
        const fn ch(a: u8, b: u8, pct: u32) -> u8 {
            ((a as u32 * (100 - pct) + b as u32 * pct + 50) / 100) as u8
        }
        Color {
            r: ch(self.r, top.r, pct),
            g: ch(self.g, top.g, pct),
            b: ch(self.b, top.b, pct),
        }
    }
    pub(crate) fn sk(self) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.r, self.g, self.b, 255)
    }
}

const WHITE: Color = Color::hex(0xFFFFFF);
const BLACK: Color = Color::hex(0x000000);

/// A colour with straight alpha: a translucent fill blended over what is already drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tint {
    pub c: Color,
    pub a: u8,
}

impl Tint {
    /// `c` at `pct` % opacity.
    pub const fn new(c: Color, pct: u32) -> Tint {
        Tint {
            c,
            a: ((if pct > 100 { 100 } else { pct } * 255 + 50) / 100) as u8,
        }
    }
    pub const fn opaque(self) -> bool {
        self.a == 255
    }
    /// The opaque result of this tint over `base` (for strips whose base is known, e.g. the
    /// keyboard window over plain BG: one opaque fill instead of a blend).
    pub const fn over(self, base: Color) -> Color {
        const fn ch(b: u8, t: u8, a: u32) -> u8 {
            ((b as u32 * (255 - a) + t as u32 * a + 127) / 255) as u8
        }
        let a = self.a as u32;
        Color {
            r: ch(base.r, self.c.r, a),
            g: ch(base.g, self.c.g, a),
            b: ch(base.b, self.c.b, a),
        }
    }
    pub(crate) fn sk(self) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.c.r, self.c.g, self.c.b, self.a)
    }
}

impl From<Color> for Tint {
    fn from(c: Color) -> Tint {
        Tint { c, a: 255 }
    }
}

/// White at `pct` % over the background.
pub const fn white(pct: u32) -> Color {
    BG.mix(WHITE, pct)
}

/// Matches the owner's coffee wallpaper (assets/wallpaper), 2026-11-10.
pub const BG: Color = Color::hex(0x393E46);
pub const TEXT: Color = WHITE;
/// App labels under icons (white 90 %).
pub const TEXT_LABEL: Color = white(90);
pub const TEXT_SECONDARY: Color = white(55);
pub const TEXT_DISABLED: Color = white(32);
/// Resting outline of buttons and icon boxes (white 60 %).
pub const OUTLINE: Color = white(60);
/// Border of cards, popovers and sheets (white 50 %).
pub const CARD_BORDER: Color = white(50);
/// Dividers, the sidebar edge, unlit signal bars (white 30 %).
pub const DIVIDER: Color = white(30);
/// Pressed state fill (white 10 %, opaque over BG); kept for opaque contexts. Surfaces use
/// `SURFACE*` + `PRESSED_TINT`.
pub const PRESSED_FILL: Color = white(10);
/// Translucent surface fills (black at N %, blended over the wallpaper):
/// cards, row groups, buttons, fields, the header pill, the dock's buttons (55 %: secondary text
/// inside a card must stay readable where the card crosses the white wallpaper art);
pub const SURFACE: Tint = Tint::new(BLACK, 55);
/// popovers (Apps panel, Quick Settings), sheets, dialogs, toasts, banners, the keyboard / pad / dock
/// strips, the sidebar's round Apps toggle (65 %: the white wallpaper art behind a panel must stay
/// well below the text);
pub const SURFACE_STRONG: Tint = Tint::new(BLACK, 65);
/// small icon boxes (Home grid, sidebar, panel icons, keys) (30 %).
pub const SURFACE_ICON: Tint = Tint::new(BLACK, 30);
/// Pressed state: drawn over the surface fill (white 12 %); the outline turns full white too.
pub const PRESSED_TINT: Tint = Tint::new(WHITE, 12);
/// The coffee-cup watermark (white 15 %).
pub const WATERMARK: Color = white(15);
/// Selected / active marks (check, caret, slider fill): plain white in the line-art theme.
pub const ACCENT: Color = WHITE;
pub const SUCCESS: Color = Color::hex(0x6DBE82);
pub const WARNING: Color = Color::hex(0xD9B25A);
pub const DANGER: Color = Color::hex(0xE0736B);

/// Spacing scale.
pub const SP: [i32; 7] = [4, 8, 12, 16, 24, 32, 48];
pub const SP_1: i32 = 4;
pub const SP_2: i32 = 8;
pub const SP_3: i32 = 12;
pub const SP_4: i32 = 16;
pub const SP_5: i32 = 24;
pub const SP_6: i32 = 32;
pub const SP_7: i32 = 48;

/// Corner radius: 12 for buttons and fields, 16 for icon boxes, cards, popovers and sheets.
pub const RADIUS: i32 = 12;
pub const RADIUS_CARD: i32 = 16;
pub const RADIUS_ICON: i32 = 16;
/// Stroke width of outlines (px).
pub const OUTLINE_W: f32 = 1.5;

/// Minimum touch target and fixed control heights.
pub const TOUCH_MIN: i32 = 56;
pub const ROW_H: i32 = 64;
pub const ROW_WIFI_H: i32 = 72;
pub const BUTTON_H: i32 = 56;
pub const ICON: i32 = 24;

/// Frame layout: sidebar on the left, header across the main area.
pub const SIDEBAR_W: i32 = 96;
pub const HEADER_H: i32 = 64;
pub const CONTENT_PAD: i32 = 24;
pub const DOCK_H: i32 = 56;
/// Icon boxes: sidebar buttons, Home grid, Apps panel.
pub const BOX_DOCK: i32 = 56;
pub const BOX_GRID: i32 = 64;
pub const BOX_PANEL: i32 = 48;

/// Type sizes in px (faces in `text::Face`).
pub const SIZE_CLOCK: f32 = 20.0;
pub const SIZE_TITLE: f32 = 26.0;
pub const SIZE_SECTION: f32 = 13.0;
pub const SIZE_BODY: f32 = 17.0;
pub const SIZE_SECONDARY: f32 = 14.0;
pub const SIZE_CAPTION: f32 = 13.0;
pub const SIZE_LABEL: f32 = 12.0;
pub const SIZE_TINY: f32 = 11.0;
pub const SIZE_MONO: f32 = 15.0;

/// Interaction thresholds (ms / px).
pub const TAP_MAX_MS: u64 = 500;
pub const LONG_PRESS_MS: u64 = 500;
pub const SLOP_PX: i32 = 10;

// Compile-time guard: every row/button is at least a minimum touch target.
const _: () = assert!(
    ROW_H >= TOUCH_MIN && BUTTON_H >= TOUCH_MIN && ROW_WIFI_H >= TOUCH_MIN && BOX_DOCK >= TOUCH_MIN
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_tokens_match_the_spec() {
        for (c, hex) in [
            (BG, "#393E46"),
            (TEXT, "#FFFFFF"),
            (TEXT_LABEL, "#EBECED"),
            (TEXT_SECONDARY, "#A6A8AC"),
            (TEXT_DISABLED, "#787C81"),
            (OUTLINE, "#B0B2B5"),
            (CARD_BORDER, "#9C9FA3"),
            (DIVIDER, "#74787E"),
            (PRESSED_FILL, "#4D5159"),
            (WATERMARK, "#575B62"),
            (ACCENT, "#FFFFFF"),
            (SUCCESS, "#6DBE82"),
            (WARNING, "#D9B25A"),
            (DANGER, "#E0736B"),
        ] {
            assert_eq!(c.to_hex(), hex);
        }
    }

    #[test]
    fn tints_are_translucent_and_flatten_like_a_blend() {
        assert_eq!(
            (SURFACE.a, SURFACE_STRONG.a, SURFACE_ICON.a),
            (140, 166, 77)
        );
        assert_eq!(PRESSED_TINT.a, 31);
        assert!(!SURFACE.opaque() && Tint::from(BG).opaque());
        assert_eq!(Tint::new(TEXT, 100).over(BG), TEXT);
        assert_eq!(Tint::new(TEXT, 0).over(BG), BG);
        // black 50 % over the charcoal halves every channel (rounded)
        assert_eq!(Tint::new(BLACK, 50).over(BG), Color::hex(0x1C1F23));
    }

    #[test]
    fn mix_is_a_rounded_linear_blend() {
        assert_eq!(BG.mix(TEXT, 0), BG);
        assert_eq!(BG.mix(TEXT, 100), TEXT);
        assert_eq!(Color::hex(0x000000).mix(Color::hex(0xFF0000), 50).r, 128);
    }

    #[test]
    fn spacing_scale_and_minimums() {
        assert_eq!(SP, [4, 8, 12, 16, 24, 32, 48]);
        const { assert!(BOX_DOCK >= TOUCH_MIN && SIDEBAR_W > BOX_DOCK + 2 * SP_3) };
        const { assert!(HEADER_H >= TOUCH_MIN) };
    }
}
