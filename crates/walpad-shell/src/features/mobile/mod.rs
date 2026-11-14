//! Mobile network screen (owner request 2026-11-10): SIM card status, the radio (airplane) and
//! mobile data switches, APN, SIM PIN entry and modem info. Model helpers are pure; content
//! coordinates as in network.rs (y starts at 0 at the top of the scrollable content).
use crate::app::Target;
use crate::device_screen::info_row;
use crate::network::switch_hit;
use walpad_proto::{MobileData, ModemState, Status};
use walpad_ui::components::{Accessory, ButtonKind, ButtonState, Painter, Tone};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Icon, Rect, Style};

mod model;
#[cfg(test)]
mod tests;

pub use model::{
    data_line, data_on, data_switchable, imei_value, layout, modem_chip, pill_cell, pin_needed,
    qs_line, radio_on, radio_switchable, sim_lines, sim_present, MobileInfo, MobileLayout,
};
use model::{mobile, registered};
pub struct View<'a> {
    pub st: &'a Option<Status>,
    pub info: &'a Option<MobileInfo>,
    pub imei_shown: bool,
    pub online: bool,
    pub pressed: Option<Target>,
}

pub fn render(p: &mut Painter, frame: &Frame, v: &View) -> Vec<(Rect, Target)> {
    let l = layout(frame, v.st);
    let mut hits = Vec::new();
    let pr = |t: Target| v.pressed == Some(t);
    let m = mobile(v.st);
    p.screen_title("Mobile network", l.title);
    let (chip, chip_col) = modem_chip(v.st);
    p.outline_box(l.chip, 0, false, false, false);
    p.cv.rect(Rect::new(l.chip.x, l.chip.y, 3, l.chip.h), chip_col);
    p.cv.rect(Rect::new(l.chip.x + 12, l.chip.y + 12, 8, 8), chip_col);
    p.text(
        chip,
        Style::new(Face::Regular, SIZE_SECONDARY, text()),
        Rect::new(l.chip.x + 28, l.chip.y, l.chip.w - 36, l.chip.h),
        Align::Left,
    );

    // SIM card
    let card = l.sim_card;
    p.card(card, false);
    p.cv.icon(Icon::Sim, card.x + SP_4, card.y + SP_4, 28, text());
    let (title, sub, sub_col) = sim_lines(v.st);
    let tx = card.x + SP_4 + 28 + SP_4;
    let tw = card.right() - tx - 120;
    p.text(
        &title,
        Style::new(Face::Bold, SIZE_BODY, text()),
        Rect::new(tx, card.y + SP_4, tw, 28),
        Align::Left,
    );
    p.text(
        &sub,
        Style::new(Face::Regular, SIZE_SECONDARY, sub_col),
        Rect::new(tx, card.y + SP_4 + 32, card.right() - tx - SP_4, 22),
        Align::Left,
    );
    let present = m.is_some_and(sim_present);
    if present {
        let lit = m.is_some_and(|m| m.radio_on == Some(true) && registered(m));
        let bars = if lit {
            m.and_then(|m| m.signal_bars).unwrap_or(0)
        } else {
            0
        };
        p.signal_bars(card.right() - SP_4 - 26, card.y + SP_4 + 4, bars, text());
        let dbm = m
            .and_then(|m| m.signal_dbm)
            .filter(|_| lit)
            .map(|d| format!("{d} dBm"))
            .unwrap_or_else(|| "No signal".into());
        p.text(
            &dbm,
            Style::new(Face::Regular, SIZE_CAPTION, text_secondary()),
            Rect::new(card.right() - 160, card.y + SP_4 + 28, 160 - SP_4, 20),
            Align::Right,
        );
    }
    let apn_in_use = m.and_then(|m| m.apn_in_use.clone());
    if let Some(a) = apn_in_use.filter(|_| m.is_some_and(|m| m.data == MobileData::On)) {
        p.text(
            &format!("Data via {a}"),
            Style::new(Face::Regular, SIZE_CAPTION, text_secondary()),
            Rect::new(tx, card.y + SP_4 + 60, card.right() - tx - SP_4, 20),
            Align::Left,
        );
    }

    if let Some(b) = l.pin_btn {
        let state = if !v.online {
            ButtonState::Disabled
        } else if pr(Target::SimPinEntry) {
            ButtonState::Pressed
        } else {
            ButtonState::Default
        };
        p.button(
            b,
            "Enter SIM PIN",
            ButtonKind::Primary,
            state,
            Some(Icon::Lock),
        );
        hits.push((b, Target::SimPinEntry));
    }

    // connection switches
    p.section_label("Connection", l.conn_label);
    p.row_group(Rect::new(
        l.radio_row.x,
        l.radio_row.y,
        l.radio_row.w,
        3 * ROW_H,
    ));
    let radio_sub = if radio_on(v.st) {
        "On"
    } else {
        "Off (airplane mode)"
    };
    let radio_dis = !v.online || !radio_switchable(v.st);
    if let Some(t) = p.list_row(
        l.radio_row,
        Some(Icon::Globe),
        "Mobile radio",
        Some(radio_sub),
        &Accessory::Toggle(radio_on(v.st)),
        pr(Target::MobileRadio),
        radio_dis,
    ) {
        hits.push((switch_hit(t), Target::MobileRadio));
    }
    hits.push((l.radio_row, Target::MobileRadio));
    let data_dis = !v.online || !data_switchable(v.st);
    let (dline, dcol) = data_line(v.st);
    let data_sub = match m.map(|m| m.data) {
        Some(MobileData::On) => "Connected",
        Some(MobileData::Connecting) => "Connecting...",
        Some(MobileData::Failed) => "Failed",
        _ => "Off",
    };
    p.list_row(
        l.data_row,
        Some(Icon::Refresh),
        "Mobile data",
        Some(data_sub),
        &Accessory::Toggle(data_on(v.st)),
        pr(Target::MobileDataSwitch),
        data_dis,
    );
    hits.push((l.data_row, Target::MobileDataSwitch));
    let apn = m
        .and_then(|m| m.apn.clone())
        .unwrap_or_else(|| "Automatic".into());
    p.list_row(
        l.apn_row,
        Some(Icon::Settings),
        "Access point (APN)",
        None,
        &Accessory::Value(apn),
        pr(Target::MobileApn),
        !v.online || m.is_none_or(|m| m.modem == ModemState::Absent),
    );
    hits.push((l.apn_row, Target::MobileApn));
    p.text(
        &dline,
        Style::new(Face::Regular, SIZE_SECONDARY, dcol),
        Rect::new(l.note.x, l.note.y, l.note.w, 22),
        Align::Left,
    );
    if m.is_some_and(|m| m.data != MobileData::On && m.radio_on == Some(true)) {
        p.text(
            "Data uses the APN above, or the operator's default",
            Style::new(Face::Regular, SIZE_CAPTION, text_secondary()),
            Rect::new(l.note.x, l.note.y + 26, l.note.w, 20),
            Align::Left,
        );
    }

    // modem info
    p.section_label("Modem", l.modem_label);
    p.row_group(Rect::new(
        l.firmware_row.x,
        l.firmware_row.y,
        l.firmware_row.w,
        2 * ROW_H,
    ));
    let (fw, tone) = match m.and_then(|m| m.firmware.clone()) {
        Some(f) => (f, Tone::Info),
        None => ("Unknown".into(), Tone::Warning),
    };
    info_row(p, l.firmware_row, "Firmware", &fw, tone);
    let imei = imei_value(v.info, v.imei_shown);
    p.list_row(
        l.imei_row,
        None,
        "IMEI",
        Some(&imei),
        &Accessory::Action(if v.imei_shown { "Hide" } else { "Show" }.into()),
        pr(Target::ImeiShow),
        !v.online || m.is_none_or(|m| m.modem != ModemState::Ready),
    );
    hits.push((l.imei_row, Target::ImeiShow));
    hits
}
