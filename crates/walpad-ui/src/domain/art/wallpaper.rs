/// The owner's coffee wallpaper ("Grey and White Coffee Desktop Wallpaper", 2026-11-10): charcoal
/// (`BG`) with sketched iced coffees and beans along the bottom, prepared for the panel in both
/// orientations.
pub const WALLPAPER_LANDSCAPE: &[u8] = include_bytes!("../../../assets/wallpaper/landscape.png");
pub const WALLPAPER_PORTRAIT: &[u8] = include_bytes!("../../../assets/wallpaper/portrait.png");

/// Decoded wallpaper for a frame of `w` x `h` (landscape when wider than tall).
pub fn wallpaper(w: i32, h: i32) -> Option<crate::canvas::Canvas> {
    crate::canvas::Canvas::from_png(if w > h {
        WALLPAPER_LANDSCAPE
    } else {
        WALLPAPER_PORTRAIT
    })
}

/// Decoded wallpaper adapted for the active theme mode (`Dark` keeps the original charcoal art;
/// `Light` maps the charcoal base and white line-art to the light background and subtle dark ink).
pub fn wallpaper_for_theme(
    w: i32,
    h: i32,
    mode: crate::tokens::ThemeMode,
) -> Option<crate::canvas::Canvas> {
    let mut cv = wallpaper(w, h)?;
    if mode == crate::tokens::ThemeMode::Light {
        let pal = crate::tokens::LIGHT_PALETTE;
        let base_r = crate::tokens::BG.r as i32;
        let span = (255 - base_r).max(1);
        for px in cv.rgba_mut().as_chunks_mut::<4>().0 {
            let t = ((px[0] as i32 - base_r).clamp(0, span) * 100 / span) as u32;
            let c = pal.bg.mix(pal.text_secondary, t * 45 / 100);
            px[0] = c.r;
            px[1] = c.g;
            px[2] = c.b;
            px[3] = 255;
        }
    }
    Some(cv)
}

/// Wallpaper mode persisted under `"wallpaper"` in `/etc/walpad/shell.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WallpaperKind {
    /// System default coffee wallpaper (`WALLPAPER_LANDSCAPE` / `WALLPAPER_PORTRAIT`).
    #[default]
    Default,
    /// System default coffee wallpaper with animated rising steam and drifting motes.
    LiveCoffee,
    /// Animated layered aurora wave ribbons.
    LiveAurora,
    /// Animated drifting & twinkling starfield.
    LiveStars,
    /// User-supplied PNG file from `"wallpaper_path"` (falls back to `Default` if missing).
    Custom,
}

impl WallpaperKind {
    pub const ALL: [WallpaperKind; 5] = [
        WallpaperKind::Default,
        WallpaperKind::LiveCoffee,
        WallpaperKind::LiveAurora,
        WallpaperKind::LiveStars,
        WallpaperKind::Custom,
    ];

    pub fn parse(s: &str) -> Self {
        match s {
            "live_coffee" => Self::LiveCoffee,
            "live_aurora" => Self::LiveAurora,
            "live_stars" => Self::LiveStars,
            "custom" => Self::Custom,
            _ => Self::Default,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::LiveCoffee => "live_coffee",
            Self::LiveAurora => "live_aurora",
            Self::LiveStars => "live_stars",
            Self::Custom => "custom",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::LiveCoffee => "Coffee",
            Self::LiveAurora => "Aurora",
            Self::LiveStars => "Stars",
            Self::Custom => "Custom",
        }
    }

    pub fn is_live(self) -> bool {
        matches!(self, Self::LiveCoffee | Self::LiveAurora | Self::LiveStars)
    }
}

/// Load a custom PNG wallpaper from `path` and aspect-fill crop it to `w` x `h`.
pub fn custom_wallpaper(path: &str, w: i32, h: i32) -> Option<crate::canvas::Canvas> {
    if path.is_empty() || w <= 0 || h <= 0 {
        return None;
    }
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > 16 * 1024 * 1024 {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let src = crate::canvas::Canvas::from_png(&bytes)?;
    let (sw, sh) = (src.width(), src.height());
    if sw <= 0 || sh <= 0 {
        return None;
    }
    if (sw, sh) == (w, h) {
        return Some(src);
    }
    let (nw, nh) = if (sw as i64) * (h as i64) >= (w as i64) * (sh as i64) {
        let nw = (((sw as i64) * (h as i64)) / (sh as i64)).max(w as i64) as i32;
        (nw, h)
    } else {
        let nh = (((sh as i64) * (w as i64)) / (sw as i64)).max(h as i64) as i32;
        (w, nh)
    };
    let scaled = src.scaled(nw as u32, nh as u32)?;
    let ox = ((nw - w) / 2).max(0);
    let oy = ((nh - h) / 2).max(0);
    scaled.crop(crate::canvas::Rect::new(ox, oy, w, h))
}

/// Render an animated live wallpaper frame onto `cv` for `phase_ms` and `mode`.
pub fn draw_live_wallpaper(
    cv: &mut crate::canvas::Canvas,
    base: Option<&crate::canvas::Canvas>,
    kind: WallpaperKind,
    phase_ms: u64,
    mode: crate::tokens::ThemeMode,
) {
    let pal = match mode {
        crate::tokens::ThemeMode::Dark => crate::tokens::DARK_PALETTE,
        crate::tokens::ThemeMode::Light => crate::tokens::LIGHT_PALETTE,
    };
    let w = cv.width() as f32;
    let h = cv.height() as f32;
    let t = (phase_ms % 600_000) as f32 * 0.001;

    match kind {
        WallpaperKind::LiveCoffee => {
            cv.fill(pal.bg);
            if let Some(b) = base {
                cv.blit(b, 0, cv.height() - b.height());
            }
            let steam_col = match mode {
                crate::tokens::ThemeMode::Dark => pal.text_secondary,
                crate::tokens::ThemeMode::Light => pal.text_disabled,
            };
            // Subtle rising steam wisps & floating motes over the lower-half coffee art.
            for i in 0..14u32 {
                let fi = i as f32;
                let cycle = ((t * (0.14 + (i % 3) as f32 * 0.03) + fi * 0.21) % 1.0).abs();
                let x = w * (0.18 + ((i * 37) % 64) as f32 * 0.01)
                    + (t * 0.8 + fi * 1.3).sin() * (12.0 + (i % 4) as f32 * 4.0);
                let y = h * (0.92 - cycle * 0.62);
                let r = if i % 3 == 0 {
                    2.5 + (1.0 - cycle) * 2.5
                } else {
                    10.0 + cycle * 26.0
                };
                let edge_fade = (cycle * std::f32::consts::PI).sin().max(0.0);
                let a = if i % 3 == 0 {
                    (edge_fade * 75.0) as u8
                } else {
                    (edge_fade * 20.0) as u8
                };
                let col = if i % 3 == 0 { pal.accent } else { steam_col };
                cv.circle(x, y, r, crate::tokens::Tint { c: col, a });
            }
        }
        WallpaperKind::LiveAurora => {
            cv.fill(pal.bg);
            let bands = [
                (0.32f32, 0.24f32, 0.45f32, pal.accent, 34u8),
                (0.50, 0.30, -0.35, pal.accent.mix(pal.text, 25), 24),
                (0.66, 0.22, 0.55, pal.text_secondary, 18),
            ];
            let steps = 16usize;
            for (idx, (cy_frac, amp_frac, speed, col, alpha)) in bands.into_iter().enumerate() {
                let mut pb = tiny_skia::PathBuilder::new();
                let base_y = h * cy_frac;
                let amp = h * amp_frac;
                let thickness = h * 0.16;
                let phase = t * speed + idx as f32 * 1.7;
                for s in 0..=steps {
                    let u = s as f32 / steps as f32;
                    let x = u * w;
                    let y = base_y
                        + (u * 4.2 + phase).sin() * amp * 0.55
                        + (u * 2.1 - phase * 0.7).cos() * amp * 0.35;
                    if s == 0 {
                        pb.move_to(x, y - thickness * 0.5);
                    } else {
                        pb.line_to(x, y - thickness * 0.5);
                    }
                }
                for s in (0..=steps).rev() {
                    let u = s as f32 / steps as f32;
                    let x = u * w;
                    let y = base_y
                        + (u * 4.2 + phase).sin() * amp * 0.55
                        + (u * 2.1 - phase * 0.7).cos() * amp * 0.35;
                    pb.line_to(x, y + thickness * 0.5);
                }
                pb.close();
                if let Some(path) = pb.finish() {
                    let mut paint = tiny_skia::Paint::default();
                    paint.set_color(crate::tokens::Tint { c: col, a: alpha }.sk());
                    paint.anti_alias = true;
                    cv.pm.fill_path(
                        &path,
                        &paint,
                        tiny_skia::FillRule::Winding,
                        tiny_skia::Transform::identity(),
                        None,
                    );
                }
            }
            for i in 0..5u32 {
                let fi = i as f32;
                let x = w * (0.15 + 0.7 * ((t * 0.07 + fi * 0.23).sin() * 0.5 + 0.5));
                let y = h * (0.22 + 0.55 * ((t * 0.05 + fi * 0.41).cos() * 0.5 + 0.5));
                cv.circle(
                    x,
                    y,
                    h * 0.14,
                    crate::tokens::Tint {
                        c: pal.accent,
                        a: 14,
                    },
                );
            }
        }
        WallpaperKind::LiveStars => {
            cv.fill(pal.bg);
            for i in 0..42u32 {
                let h1 = i.wrapping_mul(2_654_435_761);
                let h2 = h1.rotate_left(13).wrapping_mul(1_597_334_677);
                let layer = (i % 3) as f32;
                let base_x = (h1 % 1000) as f32 / 1000.0;
                let base_y = (h2 % 1000) as f32 / 1000.0;
                let drift = 0.008 + layer * 0.007;
                let x = ((base_x + t * drift) % 1.0) * w;
                let y = ((base_y + t * drift * 0.35) % 1.0) * h;
                let twinkle = (t * (1.2 + layer * 0.6) + (i as f32) * 0.9).sin() * 0.5 + 0.5;
                let r = 1.2 + layer * 0.8 + twinkle * 0.8;
                let a = (24.0 + layer * 18.0 + twinkle * 68.0) as u8;
                let col = if i % 7 == 0 {
                    pal.accent
                } else {
                    pal.text_secondary
                };
                cv.circle(x, y, r, crate::tokens::Tint { c: col, a });
            }
        }
        WallpaperKind::Default | WallpaperKind::Custom => {
            cv.fill(pal.bg);
            if let Some(b) = base {
                cv.blit(b, 0, cv.height() - b.height());
            }
        }
    }
}
