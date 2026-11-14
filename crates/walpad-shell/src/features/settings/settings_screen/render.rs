//! Layout and rendering for the Settings screen and subpages.

use super::{title, Page, Row, CATEGORIES};
use crate::app::Target;
use crate::device_screen::{info_row, tone_color};
use walpad_ui::components::{Accessory, ButtonKind, ButtonState, Painter};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Icon, Rect, Style};

pub fn row_h(r: &Row) -> i32 {
    match r {
        Row::Mono(_) => 22,
        Row::Slider { .. } => crate::volume_ui::SLIDER_ROW_H,
        _ => ROW_H,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsLayout {
    pub back: Option<Rect>,
    pub title: Rect,
    pub rows: Vec<(Rect, Row)>,
    pub total: i32,
}

pub fn layout(frame: &Frame, page: Option<Page>, rows: &[Row]) -> SettingsLayout {
    let c = frame.padded();
    let mut y = SP_4;
    let (back, title);
    match page {
        None => {
            back = None;
            title = Rect::new(c.x, y, c.w, 40);
            y += 40 + SP_4;
        }
        Some(_) => {
            let b = Rect::new(c.x, y, c.w, BUTTON_H);
            back = Some(b);
            title = Rect::new(c.x + TOUCH_MIN + SP_2, y, c.w - TOUCH_MIN - SP_2, BUTTON_H);
            y += BUTTON_H + SP_3;
        }
    }
    let mut out = Vec::new();
    if page.is_none() {
        for (i, _) in CATEGORIES.iter().enumerate() {
            out.push((
                Rect::new(c.x, y + i as i32 * ROW_H, c.w, ROW_H),
                Row::Link(CATEGORIES[i].1, CATEGORIES[i].0),
            ));
        }
        y += CATEGORIES.len() as i32 * ROW_H;
    } else {
        for r in rows {
            let h = row_h(r);
            out.push((Rect::new(c.x, y, c.w, h), r.clone()));
            y += h;
        }
    }
    SettingsLayout {
        back,
        title,
        rows: out,
        total: y + SP_6,
    }
}

fn render_action(
    p: &mut Painter,
    r: Rect,
    label: &str,
    target: Target,
    danger: bool,
    online: bool,
    pressed: Option<Target>,
) -> (Rect, Target) {
    let inner = Rect::new(r.x, r.y + 4, r.w, r.h - 8);
    let st = if !online {
        ButtonState::Disabled
    } else if pressed == Some(target) {
        ButtonState::Pressed
    } else {
        ButtonState::Default
    };
    p.button(
        inner,
        label,
        if danger {
            ButtonKind::Danger
        } else {
            ButtonKind::Secondary
        },
        st,
        None,
    );
    (inner, target)
}

pub fn render(
    p: &mut Painter,
    frame: &Frame,
    page: Option<Page>,
    rows: &[Row],
    online: bool,
    pressed: Option<Target>,
) -> Vec<(Rect, Target)> {
    let l = layout(frame, page, rows);
    let mut hits = Vec::new();
    match (page, l.back) {
        (Some(pg), Some(b)) => {
            let back_rect = Rect::new(b.x, b.y, TOUCH_MIN, b.h);
            p.outline_box(back_rect, 0, pressed == Some(Target::Back), false, false);
            p.cv.icon(
                Icon::Back,
                back_rect.x + (back_rect.w - ICON) / 2,
                back_rect.y + (back_rect.h - ICON) / 2,
                ICON,
                text(),
            );
            p.screen_title(title(pg), l.title);
            hits.push((back_rect, Target::Back));
        }
        _ => p.screen_title("Settings", l.title),
    }
    // rows over the wallpaper get a translucent card behind each run (buttons have their own fill)
    let runs = walpad_ui::components::row_runs(l.rows.iter().map(|(r, row)| {
        (
            *r,
            !matches!(row, Row::Action { .. } | Row::ActionText { .. }),
        )
    }));
    for g in runs {
        p.row_group(g);
    }
    for (r, row) in &l.rows {
        match row {
            Row::Info(label, value, tone) => {
                info_row(p, *r, label, value, *tone);
                let hit = match label.as_str() {
                    "IP address" => Some(Target::NetIpEdit),
                    "Gateway" => Some(Target::NetGatewayEdit),
                    "DNS" => Some(Target::VpnDns),
                    "MAC address" => Some(Target::NetMacEdit),
                    "Hostname" => Some(Target::NetHostnameEdit),
                    "Network" => Some(Target::WifiManualSsid),
                    "Wi-Fi state" => Some(Target::WifiSwitch),
                    "Signal" => Some(Target::Scan),
                    _ => None,
                };
                if let Some(t) = hit {
                    if pressed == Some(t) {
                        p.cv.rect(*r, pressed_tint());
                    }
                    hits.push((*r, t));
                }
            }
            Row::Choice(label, value, t) => {
                p.list_row(
                    *r,
                    None,
                    label,
                    None,
                    &Accessory::Value(value.clone()),
                    pressed == Some(*t),
                    !online,
                );
                hits.push((*r, *t));
            }
            Row::Toggle(label, on, t) => {
                p.list_row(
                    *r,
                    None,
                    label,
                    None,
                    &Accessory::Toggle(*on),
                    pressed == Some(*t),
                    !online,
                );
                hits.push((*r, *t));
            }
            Row::Action {
                label,
                target,
                danger,
            } => hits.push(render_action(
                p, *r, label, *target, *danger, online, pressed,
            )),
            Row::ActionText {
                label,
                target,
                danger,
            } => hits.push(render_action(
                p, *r, label, *target, *danger, online, pressed,
            )),
            Row::Link(label, pg) => {
                let icon = CATEGORIES
                    .iter()
                    .find(|(c, _, _)| c == pg)
                    .map(|(_, _, i)| *i);
                p.list_row(
                    *r,
                    icon,
                    label,
                    None,
                    &Accessory::Chevron,
                    pressed == Some(Target::Page(*pg)),
                    false,
                );
                hits.push((*r, Target::Page(*pg)));
            }
            Row::Slider {
                icon,
                label,
                pct,
                value,
                active,
                target,
                disabled,
            } => {
                let lc = if *disabled { text_disabled() } else { text() };
                let cap = Rect::new(r.x + SP_3, r.y + 8, r.w - 2 * SP_3, 24);
                p.cv.icon(*icon, cap.x, cap.y, ICON, lc);
                p.text(
                    label,
                    Style::new(Face::Regular, SIZE_SECONDARY + 1.0, lc),
                    Rect::new(cap.x + ICON + SP_3, cap.y, cap.w - ICON - SP_3, cap.h),
                    Align::Left,
                );
                p.text(
                    pct,
                    Style::new(Face::Regular, SIZE_SECONDARY, text_secondary()),
                    cap,
                    Align::Right,
                );
                let sl = Rect::new(r.x, cap.bottom(), r.w, 56);
                p.slider(sl, *value, *active, *disabled);
                hits.push((sl, *target));
            }
            Row::Mono(line) => p.text(
                line,
                Style::new(Face::Mono, 13.0, text_secondary()),
                Rect::new(r.x + SP_2, r.y, r.w - SP_4, r.h),
                Align::Left,
            ),
        }
    }
    let _ = tone_color;
    hits
}
