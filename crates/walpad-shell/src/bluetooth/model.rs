use walpad_proto::{BluetoothController, BluetoothDevice, BluetoothStatus, Status};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Color, Icon, Rect};

const TITLE_H: i32 = 40;
const LABEL_H: i32 = 24;
const SWITCH_W: i32 = 56;
pub(super) const ACTION_W: i32 = 132;
pub(super) const FORGET_W: i32 = 96;
const STATE_CARD_H: i32 = 240;

/// The pairing dialog's state (what the core's `bluetooth_pair` notices said).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairState {
    /// Started, waiting for the device.
    Waiting,
    /// Both sides show this passkey; the owner confirms or rejects.
    Confirm(String),
    /// Type this passkey on the device (keyboards).
    Display(String),
    Failed(String),
}

pub(super) fn bt(st: &Option<Status>) -> Option<&BluetoothStatus> {
    st.as_ref().map(|s| &s.bluetooth)
}

/// The controller answers and is powered (the header pill shows the mark).
pub fn powered(st: &Option<Status>) -> bool {
    bt(st).is_some_and(|b| b.controller == BluetoothController::Present && b.powered)
}

/// Bluetooth exists on this system (no "not installed" reason).
pub fn supported(st: &Option<Status>) -> bool {
    bt(st).is_some_and(|b| {
        !(b.controller == BluetoothController::Absent
            && b.reason
                .as_deref()
                .is_some_and(|r| r.contains("not installed")))
    })
}

/// The switch shows "on" while powered or starting up on the owner's request.
pub fn switch_on(st: &Option<Status>) -> bool {
    bt(st).is_some_and(|b| {
        b.enabled
            && (b.controller == BluetoothController::Starting
                || (b.controller == BluetoothController::Present && b.powered))
    })
}

pub fn switchable(st: &Option<Status>) -> bool {
    supported(st) && bt(st).is_some_and(|b| b.controller != BluetoothController::Starting)
}

pub fn scanning(st: &Option<Status>) -> bool {
    bt(st).is_some_and(|b| b.discovering)
}

fn connected_count(b: &BluetoothStatus) -> usize {
    b.devices.iter().filter(|d| d.connected).count()
}

/// Chip on the screen title.
pub fn chip(st: &Option<Status>) -> (&'static str, Color) {
    let Some(b) = bt(st) else {
        return ("Unknown", text_disabled());
    };
    match b.controller {
        BluetoothController::Starting => ("Starting", warning()),
        BluetoothController::Present if b.powered => ("On", success()),
        BluetoothController::Present => ("Off", text_secondary()),
        BluetoothController::Absent if !supported(st) => ("None", text_disabled()),
        BluetoothController::Absent if !b.enabled => ("Off", text_secondary()),
        BluetoothController::Absent if b.reason.is_some() => ("Failed", danger()),
        BluetoothController::Absent => ("Stopped", text_secondary()),
    }
}

/// Quick Settings row subtitle.
pub fn qs_line(st: &Option<Status>) -> (String, Color) {
    let Some(b) = bt(st) else {
        return ("Unknown".into(), text_secondary());
    };
    match b.controller {
        BluetoothController::Starting => ("Starting...".into(), warning()),
        BluetoothController::Present if b.powered => {
            let n = connected_count(b);
            if b.discovering {
                ("Scanning...".into(), text_secondary())
            } else if n == 1 {
                let name = b
                    .devices
                    .iter()
                    .find(|d| d.connected)
                    .map(|d| d.name.clone())
                    .unwrap_or_default();
                (format!("Connected: {name}"), success())
            } else if n > 1 {
                (format!("{n} devices connected"), success())
            } else {
                ("On".into(), text_secondary())
            }
        }
        BluetoothController::Present => ("Off".into(), text_secondary()),
        BluetoothController::Absent if !supported(st) => ("Not available".into(), text_disabled()),
        BluetoothController::Absent if !b.enabled => ("Off".into(), text_secondary()),
        BluetoothController::Absent if b.reason.is_some() => ("Failed to start".into(), danger()),
        BluetoothController::Absent => ("Not started".into(), text_secondary()),
    }
}

/// Stable order (never by signal: rows must not jump under a finger).
fn by_name(a: &&BluetoothDevice, b: &&BluetoothDevice) -> std::cmp::Ordering {
    a.name
        .to_lowercase()
        .cmp(&b.name.to_lowercase())
        .then(a.address.cmp(&b.address))
}

pub fn my_devices(st: &Option<Status>) -> Vec<&BluetoothDevice> {
    let mut v: Vec<_> = bt(st)
        .map(|b| b.devices.iter().filter(|d| d.paired).collect())
        .unwrap_or_default();
    v.sort_by(by_name);
    v
}

pub fn available_devices(st: &Option<Status>) -> Vec<&BluetoothDevice> {
    let mut v: Vec<_> = bt(st)
        .map(|b| b.devices.iter().filter(|d| !d.paired).collect())
        .unwrap_or_default();
    v.sort_by(by_name);
    v
}

pub fn device_icon(d: &BluetoothDevice) -> Icon {
    match d.icon.as_deref() {
        Some("input-keyboard") => Icon::Keyboard,
        Some("input-mouse" | "input-tablet" | "input-gaming") => Icon::Mouse,
        _ => Icon::Bluetooth,
    }
}

/// Four-step signal from RSSI (dBm).
pub fn bars_from_rssi(rssi: Option<i32>) -> u8 {
    match rssi {
        Some(r) if r >= -55 => 4,
        Some(r) if r >= -67 => 3,
        Some(r) if r >= -78 => 2,
        Some(_) => 1,
        None => 0,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BtRow {
    pub row: Rect,
    pub forget: Option<Rect>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BtLayout {
    pub title: Rect,
    pub chip: Rect,
    pub switch: Rect,
    pub hint: Option<Rect>,
    /// Off / absent / starting: one card with the state instead of the lists.
    pub state_card: Option<Rect>,
    pub mine_label: Rect,
    pub mine_card: Rect,
    pub mine_rows: Vec<BtRow>,
    pub avail_label: Rect,
    pub scan_btn: Rect,
    pub avail_card: Rect,
    pub avail_rows: Vec<BtRow>,
    pub total: i32,
}

pub fn layout(frame: &Frame, st: &Option<Status>) -> BtLayout {
    let c = frame.padded();
    let mut y = SP_4;
    let switch = Rect::new(c.right() - SWITCH_W, y + 4, SWITCH_W, 32);
    let chip = Rect::new(switch.x - SP_5 - 120, y + 4, 120, 32);
    let title = Rect::new(c.x, y, chip.x - SP_3 - c.x, TITLE_H);
    y += TITLE_H + SP_2;
    let reason = bt(st).and_then(|b| b.reason.as_deref()).is_some();
    let hint = reason.then(|| {
        let r = Rect::new(c.x, y, c.w, 24);
        y += 24 + SP_2;
        r
    });
    y += SP_2;
    let none = Rect::new(c.x, y, 0, 0);
    if !powered(st) {
        let card = Rect::new(c.x, y, c.w, STATE_CARD_H);
        return BtLayout {
            title,
            chip,
            switch,
            hint,
            state_card: Some(card),
            mine_label: none,
            mine_card: none,
            mine_rows: Vec::new(),
            avail_label: none,
            scan_btn: none,
            avail_card: none,
            avail_rows: Vec::new(),
            total: card.bottom() + SP_6,
        };
    }
    let rows = |y0: i32, n: usize, forget: bool| -> Vec<BtRow> {
        (0..n as i32)
            .map(|i| {
                let row = Rect::new(c.x, y0 + i * ROW_WIFI_H, c.w, ROW_WIFI_H);
                BtRow {
                    row,
                    forget: forget
                        .then(|| Rect::new(row.right() - FORGET_W, row.y, FORGET_W, row.h)),
                }
            })
            .collect()
    };
    let mine = my_devices(st).len();
    let mine_label = Rect::new(c.x, y, c.w, LABEL_H);
    y += LABEL_H + SP_2;
    let mine_h = if mine == 0 {
        56
    } else {
        mine as i32 * ROW_WIFI_H
    };
    let mine_card = Rect::new(c.x, y, c.w, mine_h);
    let mine_rows = rows(y, mine, true);
    y += mine_h + SP_5;
    let avail = available_devices(st).len();
    let avail_label = Rect::new(c.x, y, c.w - 220, LABEL_H);
    let scan_btn = Rect::new(c.right() - 200, y - 16, 200, BUTTON_H);
    y += BUTTON_H + SP_2;
    let avail_h = if avail == 0 {
        160
    } else {
        avail as i32 * ROW_WIFI_H
    };
    let avail_card = Rect::new(c.x, y, c.w, avail_h);
    let avail_rows = rows(y, avail, false);
    y += avail_h;
    BtLayout {
        title,
        chip,
        switch,
        hint,
        state_card: None,
        mine_label,
        mine_card,
        mine_rows,
        avail_label,
        scan_btn,
        avail_card,
        avail_rows,
        total: y + SP_6,
    }
}

/// Everything the screen draws from.
pub struct View<'a> {
    pub st: &'a Option<Status>,
    pub online: bool,
    pub pressed: Option<crate::app::Target>,
    pub spinner_deg: u16,
    /// Address of the connect / pair request in progress.
    pub busy: Option<&'a str>,
}
