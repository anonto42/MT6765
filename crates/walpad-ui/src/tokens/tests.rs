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
