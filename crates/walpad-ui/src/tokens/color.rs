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

pub(super) const WHITE: Color = Color::hex(0xFFFFFF);
pub(super) const BLACK: Color = Color::hex(0x000000);

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
/// The page surface behind every full-screen page's content (black 45 %, one blended rect drawn
/// into the cached background layer): the wallpaper art stays visible at the edges and through
/// the gaps between cards; rows and cards add their own `SURFACE` on top.
pub const PAGE_SURFACE: Tint = Tint::new(BLACK, 45);
/// Margin between the page surface and the content area edge / screen bottom (px).
pub const PAGE_MARGIN: i32 = 0;
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
