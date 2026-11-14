//! Device tab: display, battery, touch and hardware status. Content coordinates.
//! Unavailable hardware is labelled unavailable; nothing fake is shown as a working control.
use crate::app::Target;
use crate::config::DevdSettings;
use walpad_proto::Status;
use walpad_ui::components::Painter;
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Icon, Rect, Style};

mod model;
#[cfg(test)]
mod tests;

use model::selected_rotation;
pub use model::*;
#[allow(clippy::too_many_arguments)]
pub fn render(
    p: &mut Painter,
    frame: &Frame,
    st: &Option<Status>,
    d: &DevdSettings,
    online: bool,
    brightness: f32,
    dragging: bool,
    wallpaper: walpad_ui::art::WallpaperKind,
    wallpaper_path: &str,
    pressed: Option<Target>,
) -> Vec<(Rect, Target)> {
    let (placed, _) = layout(frame, st, d);
    let mut hits = Vec::new();
    let pr = |t: Target| pressed == Some(t);
    let rot = selected_rotation(st);
    // a translucent card behind every run of list rows (rotation, cover toggle, info)
    let runs = walpad_ui::components::row_runs(placed.iter().map(|pl| {
        (
            pl.rect,
            matches!(
                pl.item,
                Item::RotationRow(_)
                    | Item::RotateFlipRow
                    | Item::LidRow
                    | Item::ThemeRow
                    | Item::WallpaperPathRow
                    | Item::Info(..)
            ),
        )
    }));
    for g in runs {
        p.row_group(g);
    }
    for Placed { item, rect: r } in &placed {
        let r = *r;
        match item {
            Item::Title => p.screen_title("Device", Rect::new(r.x, r.y, r.w, 40)),
            Item::Label(l) => p.section_label(l, Rect::new(r.x, r.y, r.w, 24)),
            Item::SegLabel(l) => {
                let text = if *l == "Wallpaper" {
                    match wallpaper {
                        walpad_ui::art::WallpaperKind::Default => {
                            "Wallpaper · System default (Coffee)"
                        }
                        walpad_ui::art::WallpaperKind::LiveCoffee
                        | walpad_ui::art::WallpaperKind::LiveAurora
                        | walpad_ui::art::WallpaperKind::LiveStars => "Wallpaper · Live",
                        walpad_ui::art::WallpaperKind::Custom => "Wallpaper · Custom PNG",
                    }
                } else {
                    *l
                };
                p.text(
                    text,
                    Style::new(Face::Bold, SIZE_SECONDARY, text_secondary()),
                    Rect::new(r.x + 4, r.y, r.w, 28),
                    Align::Left,
                );
            }
            Item::Gap => {}
            Item::BrightnessCard => {
                let card = Rect::new(r.x, r.y, r.w, 112);
                p.card(card, false);
                let inner = card.inset(SP_4);
                p.text(
                    "Brightness",
                    Style::new(Face::Regular, SIZE_BODY, text()),
                    Rect::new(inner.x, inner.y, inner.w, 24),
                    Align::Left,
                );
                let pct = if st.is_some() || dragging {
                    format!("{}%", (brightness * 100.0).round() as i32)
                } else {
                    "--".into()
                };
                p.text(
                    &pct,
                    Style::new(Face::Regular, SIZE_BODY, text_secondary()),
                    Rect::new(inner.x, inner.y, inner.w, 24),
                    Align::Right,
                );
                let slider = Rect::new(card.x, inner.y + 28, card.w, 56);
                p.slider(slider, brightness, dragging, !online);
                hits.push((slider, Target::Slider));
            }
            Item::RotationRow(i) => {
                let (label, _) = ROTATION_ROWS[*i];
                if pr(Target::Rot(*i)) && online {
                    p.row_pressed(r);
                }
                p.divider(r.x, r.bottom() - 1, r.w);
                let on = rot == Some(*i);
                p.text(
                    label,
                    Style::new(
                        if on { Face::Bold } else { Face::Regular },
                        SIZE_BODY,
                        if online { text() } else { text_disabled() },
                    ),
                    Rect::new(r.x + SP_4, r.y, r.w - 2 * SP_4 - ICON, r.h),
                    Align::Left,
                );
                if on {
                    p.cv.icon(
                        Icon::Check,
                        r.right() - SP_4 - ICON,
                        r.y + (r.h - ICON) / 2,
                        ICON,
                        accent(),
                    );
                }
                hits.push((r, Target::Rot(*i)));
            }
            Item::RotateFlipRow => {
                let toggle = p.list_row(
                    r,
                    None,
                    "Allow upside-down rotation",
                    None,
                    &walpad_ui::components::Accessory::Toggle(d.rotate_flip),
                    pr(Target::RotateFlipToggle),
                    !online,
                );
                if toggle.is_some() {
                    hits.push((r, Target::RotateFlipToggle));
                }
            }
            Item::DimSeg | Item::OffSeg => {
                let is_dim = *item == Item::DimSeg;
                let (opts, cur): (&[(u32, &str)], u32) = if is_dim {
                    (&DIM_OPTIONS, d.dim_after_s)
                } else {
                    (&OFF_OPTIONS, d.off_after_s)
                };
                let labels: Vec<&str> = opts.iter().map(|(_, l)| *l).collect();
                let seg = Rect::new(r.x, r.y, r.w, 56);
                let sel = selected_option(opts, cur).unwrap_or(usize::MAX);
                let rects = p.segmented(seg, &labels, sel, None, !online);
                for (i, sr) in rects.into_iter().enumerate() {
                    hits.push((
                        sr,
                        if is_dim {
                            Target::Dim(i)
                        } else {
                            Target::Off(i)
                        },
                    ));
                }
            }
            Item::LidRow => {
                let toggle = p.list_row(
                    r,
                    None,
                    "Turn off when cover closes",
                    None,
                    &walpad_ui::components::Accessory::Toggle(d.lid_off),
                    pr(Target::LidToggle),
                    !online,
                );
                if toggle.is_some() {
                    hits.push((r, Target::LidToggle));
                }
            }
            Item::ThemeRow => {
                let dark = theme() == ThemeMode::Dark;
                let toggle = p.list_row(
                    r,
                    None,
                    "Dark theme",
                    None,
                    &walpad_ui::components::Accessory::Toggle(dark),
                    pr(Target::ThemeToggle),
                    !online,
                );
                if toggle.is_some() {
                    hits.push((r, Target::ThemeToggle));
                }
            }
            Item::WallpaperSeg => {
                use walpad_ui::art::WallpaperKind;
                let labels: Vec<&str> = WallpaperKind::ALL.iter().map(|k| k.label()).collect();
                let seg = Rect::new(r.x, r.y, r.w, 56);
                let sel = WallpaperKind::ALL
                    .iter()
                    .position(|&k| k == wallpaper)
                    .unwrap_or(0);
                let rects = p.segmented(seg, &labels, sel, None, !online);
                for (i, sr) in rects.into_iter().enumerate() {
                    hits.push((sr, Target::Wallpaper(i)));
                }
            }
            Item::WallpaperPathRow => {
                let sub = if wallpaper_path.is_empty() {
                    "Tap to set PNG path (e.g. /root/wall.png)"
                } else {
                    wallpaper_path
                };
                p.list_row(
                    r,
                    None,
                    "Custom wallpaper PNG",
                    Some(sub),
                    &walpad_ui::components::Accessory::Chevron,
                    pr(Target::WallpaperPathEdit),
                    !online,
                );
                hits.push((r, Target::WallpaperPathEdit));
            }
            Item::Info(l, v, t) => {
                info_row(p, r, l, v, *t);
                if l == "Low battery below" {
                    hits.push((r, Target::LowBattCycle));
                } else if l == "Shuts down below" {
                    hits.push((r, Target::CritBattCycle));
                }
            }
        }
    }
    hits
}
