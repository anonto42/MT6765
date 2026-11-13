//! Bluetooth screen (owner request 2026-11-10): switch, scan, My devices (paired: connect /
//! disconnect / forget), Available devices (tap to pair) and the pairing dialog. Model helpers and
//! layout are pure; content coordinates as in network.rs (y starts at 0 at the top of the
//! scrollable content).
use crate::app::Target;
use crate::network::switch_hit;
use walpad_proto::{BluetoothController, BluetoothDevice, BluetoothStatus, Status};
use walpad_ui::components::{ButtonKind, ButtonState, Painter};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Color, Face, Icon, Rect, Style};

const TITLE_H: i32 = 40;
const LABEL_H: i32 = 24;
const SWITCH_W: i32 = 56;
const ACTION_W: i32 = 132;
const FORGET_W: i32 = 96;
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

fn bt(st: &Option<Status>) -> Option<&BluetoothStatus> {
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
    pub pressed: Option<Target>,
    pub spinner_deg: u16,
    /// Address of the connect / pair request in progress.
    pub busy: Option<&'a str>,
}

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

// ------------------------------------------------------------------------ pairing dialog
/// Geometry of the 560 x 260 dialog (same as `Painter::dialog`): (dialog, buttons).
fn dialog_rect(screen: Rect) -> Rect {
    let w = 560.min(screen.w - 2 * SP_5);
    let h = 260;
    Rect::new(
        screen.x + (screen.w - w) / 2,
        screen.y + (screen.h - h) / 2,
        w,
        h,
    )
}

pub fn render_pair(
    p: &mut Painter,
    frame: &Frame,
    name: &str,
    state: &PairState,
    spinner_deg: u16,
) -> Vec<(Rect, Target)> {
    let screen = Rect::new(0, 0, frame.w, frame.h);
    let r = dialog_rect(screen);
    p.popover(r);
    let title = match state {
        PairState::Waiting => "Pairing".to_string(),
        PairState::Confirm(_) => format!("Pair with {name}?"),
        PairState::Display(_) => format!("Type this on {name}"),
        PairState::Failed(_) => "Pairing failed".to_string(),
    };
    p.text(
        &title,
        Style::new(Face::Regular, SIZE_TITLE, text()),
        Rect::new(r.x + SP_5, r.y + SP_5, r.w - 2 * SP_5, 40),
        Align::Left,
    );
    let body_r = Rect::new(r.x + SP_5, r.y + 68, r.w - 2 * SP_5, 28);
    let small = |p: &mut Painter, s: &str, c: Color, rr: Rect| {
        p.text(
            s,
            Style::new(Face::Regular, SIZE_SECONDARY, c),
            rr,
            Align::Left,
        )
    };
    let big = |p: &mut Painter, s: &str| {
        p.text(
            s,
            Style::new(Face::Mono, 36.0, text()),
            Rect::new(r.x + SP_5, r.y + 104, r.w - 2 * SP_5, 48),
            Align::Center,
        )
    };
    match state {
        PairState::Waiting => {
            small(
                p,
                &format!("Connecting to {name}..."),
                text_secondary(),
                body_r,
            );
            p.spinner(r.x + r.w / 2, r.y + 130, 32, spinner_deg as f32);
        }
        PairState::Confirm(code) => {
            small(
                p,
                "Does the other device show this number?",
                text_secondary(),
                body_r,
            );
            big(p, code);
        }
        PairState::Display(code) => {
            small(
                p,
                "then press Enter. Pairing finishes by itself.",
                text_secondary(),
                body_r,
            );
            big(p, code);
        }
        PairState::Failed(why) => small(
            p,
            why,
            danger(),
            Rect::new(body_r.x, body_r.y, body_r.w, 56),
        ),
    }
    let by = r.bottom() - SP_5 - BUTTON_H;
    let mut hits = Vec::new();
    match state {
        PairState::Confirm(_) => {
            let bw = (r.w - 2 * SP_5 - SP_4) / 2;
            let no = Rect::new(r.x + SP_5, by, bw, BUTTON_H);
            let yes = Rect::new(no.right() + SP_4, by, bw, BUTTON_H);
            p.button(
                no,
                "Don't pair",
                ButtonKind::Secondary,
                ButtonState::Default,
                None,
            );
            p.button(yes, "Pair", ButtonKind::Primary, ButtonState::Default, None);
            hits.push((no, Target::BtPairReject));
            hits.push((yes, Target::BtPairAccept));
        }
        other => {
            let b = Rect::new(r.x + SP_5, by, r.w - 2 * SP_5, BUTTON_H);
            let label = match other {
                PairState::Failed(_) => "OK",
                PairState::Waiting => "Close",
                _ => "Cancel",
            };
            p.button(b, label, ButtonKind::Secondary, ButtonState::Default, None);
            hits.push((b, Target::SheetCancel));
        }
    }
    hits
}

pub fn render_forget(p: &mut Painter, frame: &Frame, name: &str) -> Vec<(Rect, Target)> {
    let screen = Rect::new(0, 0, frame.w, frame.h);
    let (cancel, confirm) = p.dialog(
        screen,
        "Forget device?",
        &format!("{name} will be unpaired. You can pair it again later."),
        "Cancel",
        "Forget",
        "Forget",
        true,
        false,
    );
    vec![
        (cancel, Target::DialogCancel),
        (confirm, Target::DialogConfirm),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(a: &str, name: &str, paired: bool, connected: bool) -> BluetoothDevice {
        BluetoothDevice {
            address: a.into(),
            name: name.into(),
            paired,
            connected,
            ..Default::default()
        }
    }

    #[test]
    fn lists_are_split_and_stably_ordered() {
        let st = crate::app::tests::status(1, &[]);
        let mut st = st;
        st.bluetooth = BluetoothStatus {
            controller: BluetoothController::Present,
            powered: true,
            devices: vec![
                dev("00:00:00:00:00:03", "zeta", true, false),
                dev("00:00:00:00:00:02", "Alpha", true, true),
                dev("00:00:00:00:00:05", "phone", false, false),
                dev("00:00:00:00:00:04", "Phone", false, false),
            ],
            ..Default::default()
        };
        let st = Some(st);
        let mine: Vec<_> = my_devices(&st).iter().map(|d| d.name.as_str()).collect();
        assert_eq!(mine, ["Alpha", "zeta"]);
        let av: Vec<_> = available_devices(&st)
            .iter()
            .map(|d| d.address.as_str())
            .collect();
        assert_eq!(av, ["00:00:00:00:00:04", "00:00:00:00:00:05"]);
        assert!(powered(&st) && switch_on(&st) && switchable(&st));
        assert_eq!(qs_line(&st).0, "Connected: Alpha");
    }

    #[test]
    fn controller_states_map_to_lines() {
        let mut s = crate::app::tests::status(1, &[]);
        let line = |s: &Status, f: &dyn Fn(&mut BluetoothStatus)| {
            let mut s = s.clone();
            f(&mut s.bluetooth);
            let o = Some(s);
            (qs_line(&o).0, chip(&o).0, switch_on(&o), switchable(&o))
        };
        assert_eq!(
            line(&s, &|_| {}),
            ("Not started".into(), "Stopped", false, true)
        );
        assert_eq!(
            line(&s, &|b| b.enabled = false),
            ("Off".into(), "Off", false, true)
        );
        assert_eq!(
            line(&s, &|b| b.controller = BluetoothController::Starting),
            ("Starting...".into(), "Starting", true, false)
        );
        assert_eq!(
            line(&s, &|b| b.reason =
                Some("Bluetooth is not installed on this system".into())),
            ("Not available".into(), "None", false, false)
        );
        assert_eq!(
            line(&s, &|b| b.reason =
                Some("bridge did not create an HCI device".into())),
            ("Failed to start".into(), "Failed", false, true)
        );
        assert_eq!(
            line(&s, &|b| {
                b.controller = BluetoothController::Present;
                b.powered = true;
                b.discovering = true;
            }),
            ("Scanning...".into(), "On", true, true)
        );
        assert_eq!(
            line(&s, &|b| b.controller = BluetoothController::Present),
            ("Off".into(), "Off", false, true)
        );
        s.bluetooth.controller = BluetoothController::Present;
        assert!(!powered(&Some(s)));
    }

    #[test]
    fn touch_targets_fit_in_both_orientations() {
        let mut s = crate::app::tests::status(1, &[]);
        s.bluetooth = BluetoothStatus {
            controller: BluetoothController::Present,
            powered: true,
            devices: vec![
                dev("00:00:00:00:00:02", "Alpha", true, true),
                dev("00:00:00:00:00:04", "Phone", false, false),
            ],
            ..Default::default()
        };
        let st = Some(s);
        for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
            let l = layout(&f, &st);
            let c = f.padded();
            assert!(l.scan_btn.h >= TOUCH_MIN && l.scan_btn.w >= TOUCH_MIN);
            for r in l.mine_rows.iter().chain(&l.avail_rows) {
                assert!(r.row.h >= TOUCH_MIN && r.row.x >= c.x && r.row.right() <= c.right());
                if let Some(fg) = r.forget {
                    assert!(fg.w >= 80 && fg.h >= TOUCH_MIN && fg.right() <= r.row.right());
                }
            }
            assert!(l.avail_card.y > l.mine_card.bottom());
            assert!(l.total > l.avail_card.bottom());
        }
        // off: one state card, no lists
        let off = Some(crate::app::tests::status(1, &[]));
        let l = layout(&Frame::new(800, 1280), &off);
        assert!(l.state_card.is_some() && l.mine_rows.is_empty());
    }

    #[test]
    fn rssi_bars_and_icons() {
        assert_eq!(bars_from_rssi(Some(-50)), 4);
        assert_eq!(bars_from_rssi(Some(-90)), 1);
        assert_eq!(bars_from_rssi(None), 0);
        let mut d = dev("00:00:00:00:00:02", "k", true, false);
        d.icon = Some("input-keyboard".into());
        assert_eq!(device_icon(&d), Icon::Keyboard);
        d.icon = Some("audio-headset".into());
        assert_eq!(device_icon(&d), Icon::Bluetooth);
    }
}
