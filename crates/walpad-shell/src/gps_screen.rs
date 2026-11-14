//! GPS screen (owner request 2026-11-11): the switch, the fix state, the coordinates with Copy
//! buttons, satellite signal bars and the age of the last fix. Model helpers are pure; content
//! coordinates as in mobile.rs (y starts at 0 at the top of the scrollable content). The position is
//! private: it is drawn for the owner and copied only when a Copy button is tapped, never logged.
use crate::app::Target;
use crate::network::switch_hit;
use walpad_proto::{GpsState, Status};
use walpad_ui::components::{Accessory, ButtonKind, ButtonState, Painter};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Icon, Rect, Style};

mod model;
#[cfg(test)]
mod tests;

pub use model::{
    age_text, busy, chip, coords_text, layout, map_link, pill_mark, qs_line, state_lines,
    switch_on, switchable, GpsLayout, FRESH_S,
};
use model::{gps, SAT_CARD_H};
pub struct View<'a> {
    pub st: &'a Option<Status>,
    pub online: bool,
    pub pressed: Option<Target>,
}

/// Bar height for a signal of `snr` dB-Hz in a chart `h` px high (0 = a stub: tracked, no signal).
pub fn bar_height(snr: u8, h: i32) -> i32 {
    let f = (snr.min(50) as f32) / 50.0;
    ((f * h as f32).round() as i32).max(4)
}

pub fn render(p: &mut Painter, frame: &Frame, v: &View) -> Vec<(Rect, Target)> {
    let l = layout(frame, v.st);
    let mut hits = Vec::new();
    let pr = |t: Target| v.pressed == Some(t);
    let g = gps(v.st);
    p.outline_box(l.back, 0, pr(Target::Back), false, false);
    p.cv.icon(
        Icon::Back,
        l.back.x + (l.back.w - ICON) / 2,
        l.back.y + (l.back.h - ICON) / 2,
        ICON,
        text(),
    );
    hits.push((l.back, Target::Back));
    p.screen_title("GPS", l.title);
    let (chip, chip_col) = chip(v.st);
    p.outline_box(l.chip, 0, false, false, false);
    p.cv.rect(Rect::new(l.chip.x, l.chip.y, 3, l.chip.h), chip_col);
    p.cv.rect(Rect::new(l.chip.x + 12, l.chip.y + 12, 8, 8), chip_col);
    p.text(
        chip,
        Style::new(Face::Regular, SIZE_SECONDARY, text()),
        Rect::new(l.chip.x + 28, l.chip.y, l.chip.w - 36, l.chip.h),
        Align::Left,
    );

    // state card
    let card = l.state_card;
    p.card(card, false);
    p.cv.icon(Icon::Location, card.x + SP_4, card.y + SP_4, 28, text());
    let (title, sub, sub_col) = state_lines(v.st);
    let tx = card.x + SP_4 + 28 + SP_4;
    p.text(
        &title,
        Style::new(Face::Bold, SIZE_BODY, text()),
        Rect::new(tx, card.y + SP_4, card.right() - tx - SP_4, 28),
        Align::Left,
    );
    p.text(
        &sub,
        Style::new(Face::Regular, SIZE_SECONDARY, sub_col),
        Rect::new(tx, card.y + SP_4 + 32, card.right() - tx - SP_4, 44),
        Align::Left,
    );

    // switch
    p.row_group(l.switch_row);
    let dis = !v.online || !switchable(v.st);
    let ssub = if switch_on(v.st) { "On" } else { "Off" };
    if let Some(t) = p.list_row(
        l.switch_row,
        Some(Icon::Location),
        "GPS",
        Some(ssub),
        &Accessory::Toggle(switch_on(v.st)),
        pr(Target::GpsSwitch),
        dis,
    ) {
        hits.push((switch_hit(t), Target::GpsSwitch));
    }
    hits.push((l.switch_row, Target::GpsSwitch));

    // position
    if let (Some(lab), Some(pc), Some(g)) = (l.pos_label, l.pos_card, g) {
        p.section_label("Position", lab);
        p.card(pc, false);
        let coords = coords_text(g).unwrap_or_default();
        p.text(
            &coords,
            Style::new(Face::Mono, 22.0, text()),
            Rect::new(pc.x + SP_4, pc.y + SP_3, pc.w - 2 * SP_4, 32),
            Align::Left,
        );
        let mut parts: Vec<String> = Vec::new();
        if let Some(a) = g.accuracy_m {
            parts.push(format!("Accuracy {a} m"));
        }
        if let Some(a) = g.alt_m {
            parts.push(format!("Altitude {a} m"));
        }
        if !parts.is_empty() {
            p.text(
                &parts.join("  ·  "),
                Style::new(Face::Regular, SIZE_SECONDARY, text_secondary()),
                Rect::new(pc.x + SP_4, pc.y + 48, pc.w - 2 * SP_4, 22),
                Align::Left,
            );
        }
        if let Some(age) = g.age_s {
            let fresh = g.state == GpsState::Fix && age <= FRESH_S;
            let (text, col) = if fresh {
                (format!("Last fix {}", age_text(age)), success())
            } else {
                (format!("Last known position, {}", age_text(age)), warning())
            };
            p.text(
                &text,
                Style::new(Face::Regular, SIZE_SECONDARY, col),
                Rect::new(pc.x + SP_4, pc.y + 74, pc.w - 2 * SP_4, 22),
                Align::Left,
            );
        }
        if let Some(b) = l.copy_coords {
            let state = if pr(Target::GpsCopyCoords) {
                ButtonState::Pressed
            } else {
                ButtonState::Default
            };
            p.button(b, "Copy position", ButtonKind::Secondary, state, None);
            hits.push((b, Target::GpsCopyCoords));
        }
    }
    if let Some(b) = l.copy_link {
        let state = if pr(Target::GpsOpenMap) {
            ButtonState::Pressed
        } else {
            ButtonState::Default
        };
        p.button(b, "OpenStreetMap", ButtonKind::Primary, state, None);
        hits.push((b, Target::GpsOpenMap));
    }

    // satellites
    p.section_label("Satellites", l.sat_label);
    let sc = l.sat_card;
    p.card(sc, false);
    let signals: &[u8] = g.map(|g| g.signals.as_slice()).unwrap_or(&[]);
    let head = match g.map(|g| (g.satellites_view, g.satellites_used)) {
        Some((Some(v), Some(u))) => format!("{v} in view  ·  {u} used in the fix"),
        Some((Some(v), None)) => format!("{v} in view"),
        _ => "No satellites yet".to_string(),
    };
    p.text(
        &head,
        Style::new(Face::Regular, SIZE_BODY, text()),
        Rect::new(sc.x + SP_4, sc.y + SP_3, sc.w - 2 * SP_4, 28),
        Align::Left,
    );
    // bar chart: up to 16 bars, strongest first, 4 px stubs for satellites without a signal
    let chart = Rect::new(
        sc.x + SP_4,
        sc.y + 48,
        sc.w - 2 * SP_4,
        SAT_CARD_H - 48 - 40,
    );
    let base = chart.bottom();
    p.cv.rect(Rect::new(chart.x, base, chart.w, 1), divider());
    if !signals.is_empty() {
        let n = signals.len().min(16) as i32;
        let gap = 6;
        let bw = ((chart.w - gap * (n - 1)) / n).clamp(8, 40);
        for (i, s) in signals.iter().take(16).enumerate() {
            let h = bar_height(*s, chart.h);
            let x = chart.x + i as i32 * (bw + gap);
            let col = if *s == 0 { divider() } else { text() };
            p.cv.rect(Rect::new(x, base - h, bw, h), col);
        }
    }
    p.text(
        "Signal strength of the strongest satellites (0 to 50 dB-Hz)",
        Style::new(Face::Regular, SIZE_CAPTION, text_secondary()),
        Rect::new(sc.x + SP_4, sc.bottom() - 32, sc.w - 2 * SP_4, 24),
        Align::Left,
    );

    p.text(
        "Needs open sky and Wi-Fi on. The first fix can take minutes.",
        Style::new(Face::Regular, SIZE_CAPTION, text_secondary()),
        l.note,
        Align::Left,
    );
    hits
}
