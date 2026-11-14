use super::model::{
    available_devices, bars_from_rssi, bt, chip, device_icon, layout, my_devices, scanning,
    supported, switch_on, switchable, View, ACTION_W, FORGET_W,
};
use crate::app::Target;
use crate::network::switch_hit;
use walpad_proto::{BluetoothController, BluetoothDevice};
use walpad_ui::components::{ButtonKind, ButtonState, Painter};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Color, Face, Icon, Rect, Style};

pub fn render(p: &mut Painter, frame: &Frame, v: &View) -> Vec<(Rect, Target)> {
    let l = layout(frame, v.st);
    let mut hits = Vec::new();
    let pr = |t: Target| v.pressed == Some(t);
    p.screen_title("Bluetooth", l.title);
    let sw_off = !v.online || !switchable(v.st);
    p.toggle(
        l.switch.x,
        l.switch.y,
        switch_on(v.st),
        pr(Target::BtSwitch),
        sw_off,
    );
    hits.push((switch_hit(l.switch), Target::BtSwitch));
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
    if let (Some(h), Some(why)) = (l.hint, bt(v.st).and_then(|b| b.reason.clone())) {
        p.text(
            &why,
            Style::new(Face::Regular, SIZE_SECONDARY, danger()),
            h,
            Align::Left,
        );
    }

    if let Some(card) = l.state_card {
        p.card(card, false);
        let (title, hint) = match bt(v.st).map(|b| b.controller) {
            None => ("No status", "Waiting for the system"),
            Some(BluetoothController::Starting) => {
                ("Starting Bluetooth", "This takes up to a minute after boot")
            }
            _ if !supported(v.st) => ("Bluetooth is not available", "It is not installed"),
            Some(BluetoothController::Present) => {
                ("Bluetooth is off", "Turn it on to find devices")
            }
            Some(BluetoothController::Absent) if bt(v.st).is_some_and(|b| !b.enabled) => {
                ("Bluetooth is off", "Turn it on to find devices")
            }
            Some(BluetoothController::Absent) => {
                ("Bluetooth is not running", "Use the switch to start it")
            }
        };
        p.empty_state(card, Icon::Bluetooth, title, hint);
        return hits;
    }

    // My devices
    p.section_label("My devices", l.mine_label);
    p.card(l.mine_card, false);
    let mine = my_devices(v.st);
    if mine.is_empty() {
        p.text(
            "No paired devices",
            Style::new(Face::Regular, SIZE_BODY, text_disabled()),
            Rect::new(
                l.mine_card.x + SP_4,
                l.mine_card.y,
                l.mine_card.w - 2 * SP_4,
                l.mine_card.h,
            ),
            Align::Left,
        );
    }
    for (i, (d, r)) in mine.iter().zip(&l.mine_rows).enumerate() {
        let busy = v.busy == Some(d.address.as_str());
        let (sub, col, action) = if busy {
            ("Working...".to_string(), warning(), "")
        } else if d.connected {
            ("Connected".to_string(), success(), "Disconnect")
        } else {
            ("Paired".to_string(), text_secondary(), "Connect")
        };
        let forget = r.forget.unwrap_or(r.row);
        let text_r = Rect::new(r.row.x, r.row.y, r.row.w - FORGET_W - ACTION_W, r.row.h);
        device_row(p, r.row, text_r, d, &sub, col, pr(Target::BtAction(i)));
        if busy {
            p.spinner(
                forget.x - ACTION_W / 2,
                r.row.y + r.row.h / 2,
                24,
                v.spinner_deg as f32,
            );
        } else {
            p.text(
                action,
                Style::new(Face::Bold, SIZE_BODY, text()),
                Rect::new(forget.x - ACTION_W, r.row.y, ACTION_W - SP_2, r.row.h),
                Align::Right,
            );
        }
        p.text(
            "Forget",
            Style::new(Face::Bold, SIZE_BODY, danger()),
            Rect::new(forget.x, forget.y, forget.w - SP_4, forget.h),
            Align::Right,
        );
        hits.push((r.row, Target::BtAction(i)));
        hits.push((forget, Target::BtForget(i)));
    }

    // Available devices
    p.section_label("Available devices", l.avail_label);
    let scan_state = if scanning(v.st) {
        ButtonState::Loading(v.spinner_deg)
    } else if !v.online {
        ButtonState::Disabled
    } else if pr(Target::BtScan) {
        ButtonState::Pressed
    } else {
        ButtonState::Default
    };
    p.button(
        l.scan_btn,
        if scanning(v.st) { "Scanning" } else { "Scan" },
        ButtonKind::Secondary,
        scan_state,
        Some(Icon::Refresh),
    );
    hits.push((l.scan_btn, Target::BtScan));
    p.card(l.avail_card, false);
    let avail = available_devices(v.st);
    if avail.is_empty() {
        let (title, hint) = if scanning(v.st) {
            (
                "Looking for devices",
                "Make sure the device is discoverable",
            )
        } else {
            ("No devices found", "Tap Scan to look for devices")
        };
        p.empty_state(l.avail_card, Icon::Bluetooth, title, hint);
    }
    for (i, (d, r)) in avail.iter().zip(&l.avail_rows).enumerate() {
        let busy = v.busy == Some(d.address.as_str());
        let (sub, col) = if busy {
            ("Pairing...", warning())
        } else {
            ("Tap to pair", text_secondary())
        };
        let text_r = Rect::new(r.row.x, r.row.y, r.row.w - 80, r.row.h);
        device_row(p, r.row, text_r, d, sub, col, pr(Target::BtAvail(i)));
        if busy {
            p.spinner(
                r.row.right() - 40,
                r.row.y + r.row.h / 2,
                24,
                v.spinner_deg as f32,
            );
        } else if d.rssi.is_some() {
            p.signal_bars(
                r.row.right() - SP_4 - 26,
                r.row.y + (r.row.h - 18) / 2,
                bars_from_rssi(d.rssi),
                text(),
            );
        }
        hits.push((r.row, Target::BtAvail(i)));
    }
    hits
}

/// A 72 px device row: icon, name, status line; the caller adds the accessories at the right.
fn device_row(
    p: &mut Painter,
    row: Rect,
    text_r: Rect,
    d: &BluetoothDevice,
    sub: &str,
    sub_col: Color,
    pressed: bool,
) {
    if !p.cv.visible(row) {
        return;
    }
    if pressed {
        p.row_pressed(row);
    }
    p.divider(row.x, row.bottom() - 1, row.w);
    p.cv.icon(
        device_icon(d),
        row.x + SP_4,
        row.y + (row.h - ICON) / 2,
        ICON,
        outline(),
    );
    let tx = row.x + SP_4 + ICON + SP_4;
    let tw = text_r.right() - tx - SP_2;
    p.text(
        &d.name,
        Style::new(Face::Regular, SIZE_BODY, text()),
        Rect::new(tx, row.y + 12, tw, 28),
        Align::Left,
    );
    p.text(
        sub,
        Style::new(Face::Regular, SIZE_SECONDARY, sub_col),
        Rect::new(tx, row.y + 40, tw, 22),
        Align::Left,
    );
}
