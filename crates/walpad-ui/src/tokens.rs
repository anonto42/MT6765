//! Design tokens (docs/rust/UI.md): the line-art theme. One charcoal background, white strokes and
//! text. "White at N %" stroke/text colours are pre-blended over the background at compile time
//! (opaque writes). Surfaces (popovers, cards, rows, buttons, icon boxes, sheets, the dock, the
//! keyboard, the mouse pad) get a translucent dark fill (`Tint`, owner request 2026-11-10) that is
//! alpha-blended over whatever is beneath (the wallpaper art), so a component always reads apart
//! from the background. No gradients, no shadows. Cards, row groups and the page surface carry no
//! outline (owner request 2026-11-12, `ui-polish`): the shade difference alone separates them, the
//! way grouped sections work in iOS/Material settings; outlines stay on controls that need an edge
//! to read as tappable or as a popover/sheet floating over arbitrary content (buttons, fields,
//! icon boxes, popovers, dialogs).

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

/// Active UI theme mode (persisted in `/etc/walpad/shell.json` under `"theme"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

impl ThemeMode {
    pub fn from_str_or_dark(s: &str) -> Self {
        if s.eq_ignore_ascii_case("light") {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            ThemeMode::Dark => "dark",
            ThemeMode::Light => "light",
        }
    }
    pub fn toggle(self) -> Self {
        match self {
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
        }
    }
}

/// Complete colour palette for one theme mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub bg: Color,
    pub text: Color,
    pub text_label: Color,
    pub text_secondary: Color,
    pub text_disabled: Color,
    pub outline: Color,
    pub card_border: Color,
    pub divider: Color,
    pub pressed_fill: Color,
    pub surface: Tint,
    pub surface_strong: Tint,
    pub page_surface: Tint,
    pub surface_icon: Tint,
    pub pressed_tint: Tint,
    pub watermark: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
}

pub const DARK_PALETTE: Palette = Palette {
    bg: BG,
    text: TEXT,
    text_label: TEXT_LABEL,
    text_secondary: TEXT_SECONDARY,
    text_disabled: TEXT_DISABLED,
    outline: OUTLINE,
    card_border: CARD_BORDER,
    divider: DIVIDER,
    pressed_fill: PRESSED_FILL,
    surface: SURFACE,
    surface_strong: SURFACE_STRONG,
    page_surface: PAGE_SURFACE,
    surface_icon: SURFACE_ICON,
    pressed_tint: PRESSED_TINT,
    watermark: WATERMARK,
    accent: ACCENT,
    success: SUCCESS,
    warning: WARNING,
    danger: DANGER,
};

const LIGHT_BG: Color = Color::hex(0xF0F2F5);
const LIGHT_INK: Color = Color::hex(0x1A1D21);

pub const LIGHT_PALETTE: Palette = Palette {
    bg: LIGHT_BG,
    text: LIGHT_INK,
    text_label: LIGHT_BG.mix(LIGHT_INK, 90),
    text_secondary: LIGHT_BG.mix(LIGHT_INK, 58),
    text_disabled: LIGHT_BG.mix(LIGHT_INK, 34),
    outline: LIGHT_BG.mix(LIGHT_INK, 45),
    card_border: LIGHT_BG.mix(LIGHT_INK, 30),
    divider: LIGHT_BG.mix(LIGHT_INK, 18),
    pressed_fill: LIGHT_BG.mix(LIGHT_INK, 10),
    surface: Tint::new(WHITE, 78),
    surface_strong: Tint::new(WHITE, 88),
    page_surface: Tint::new(WHITE, 65),
    surface_icon: Tint::new(WHITE, 55),
    pressed_tint: Tint::new(BLACK, 10),
    watermark: LIGHT_BG.mix(LIGHT_INK, 12),
    accent: LIGHT_INK,
    success: Color::hex(0x2E7D4B),
    warning: Color::hex(0xB47B14),
    danger: Color::hex(0xC93B35),
};

static THEME_STATE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub fn set_theme(mode: ThemeMode) {
    let v = match mode {
        ThemeMode::Dark => 0,
        ThemeMode::Light => 1,
    };
    THEME_STATE.store(v, std::sync::atomic::Ordering::Relaxed);
}

pub fn theme() -> ThemeMode {
    if THEME_STATE.load(std::sync::atomic::Ordering::Relaxed) == 1 {
        ThemeMode::Light
    } else {
        ThemeMode::Dark
    }
}

pub fn palette() -> &'static Palette {
    match theme() {
        ThemeMode::Dark => &DARK_PALETTE,
        ThemeMode::Light => &LIGHT_PALETTE,
    }
}

pub fn bg() -> Color {
    palette().bg
}
pub fn text() -> Color {
    palette().text
}
pub fn text_label() -> Color {
    palette().text_label
}
pub fn text_secondary() -> Color {
    palette().text_secondary
}
pub fn text_disabled() -> Color {
    palette().text_disabled
}
pub fn outline() -> Color {
    palette().outline
}
pub fn card_border() -> Color {
    palette().card_border
}
pub fn divider() -> Color {
    palette().divider
}
pub fn pressed_fill() -> Color {
    palette().pressed_fill
}
pub fn surface() -> Tint {
    palette().surface
}
pub fn surface_strong() -> Tint {
    palette().surface_strong
}
pub fn page_surface() -> Tint {
    palette().page_surface
}
pub fn surface_icon() -> Tint {
    palette().surface_icon
}
pub fn pressed_tint() -> Tint {
    palette().pressed_tint
}
pub fn watermark() -> Color {
    palette().watermark
}
pub fn accent() -> Color {
    palette().accent
}
pub fn success() -> Color {
    palette().success
}
pub fn warning() -> Color {
    palette().warning
}
pub fn danger() -> Color {
    palette().danger
}

/// Spacing scale.
pub const SP: [i32; 7] = [4, 8, 12, 16, 24, 32, 48];
pub const SP_1: i32 = 4;
pub const SP_2: i32 = 8;
pub const SP_3: i32 = 12;
pub const SP_4: i32 = 16;
pub const SP_5: i32 = 24;
pub const SP_6: i32 = 32;
pub const SP_7: i32 = 48;

/// Corner radius: 0 px everywhere for the boxy geometric UI.
pub const RADIUS: i32 = 0;
pub const RADIUS_CARD: i32 = 0;
pub const RADIUS_ICON: i32 = 0;
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
        assert_eq!(PAGE_SURFACE.a, 115);
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

    #[test]
    fn dark_and_light_theme_palettes_switch() {
        assert_eq!(theme(), ThemeMode::Dark);
        assert_eq!(bg(), DARK_PALETTE.bg);
        set_theme(ThemeMode::Light);
        assert_eq!(theme(), ThemeMode::Light);
        assert_eq!(bg(), LIGHT_PALETTE.bg);
        assert_ne!(bg(), DARK_PALETTE.bg);
        assert_ne!(text(), DARK_PALETTE.text);
        set_theme(ThemeMode::Dark);
        assert_eq!(theme(), ThemeMode::Dark);
    }
}
