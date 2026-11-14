use super::color::*;

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
