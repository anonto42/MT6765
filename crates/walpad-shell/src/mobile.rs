//! Mobile network screen (owner request 2026-11-10): SIM card status, the radio (airplane) and
//! mobile data switches, APN, SIM PIN entry and modem info. Model helpers are pure; content
//! coordinates as in network.rs (y starts at 0 at the top of the scrollable content).
use crate::app::Target;
use crate::device_screen::info_row;
use crate::network::switch_hit;
use walpad_proto::{MobileData, MobileReg, MobileStatus, ModemState, SimState, Status};
use walpad_ui::components::{Accessory, ButtonKind, ButtonState, Painter, Tone};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Color, Face, Icon, Rect, Style};

const TITLE_H: i32 = 40;
const SIM_CARD_H: i32 = 112;
const LABEL_H: i32 = 24;
const NOTE_H: i32 = 52;

/// The identifiers `mobile_info` returned (shown on request only, never logged).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MobileInfo {
    pub imei: Option<String>,
    pub iccid: Option<String>,
}

fn mobile(st: &Option<Status>) -> Option<&MobileStatus> {
    st.as_ref().map(|s| &s.mobile)
}

/// A SIM sits in the tray (whatever its lock state).
pub fn sim_present(m: &MobileStatus) -> bool {
    m.modem == ModemState::Ready
        && matches!(
            m.sim,
            SimState::Ready | SimState::PinRequired | SimState::PukRequired | SimState::Busy
        )
}

fn registered(m: &MobileStatus) -> bool {
    matches!(m.registration, MobileReg::Home | MobileReg::Roaming)
}

/// Header pill: signal bars while a SIM is present (white when registered, dim otherwise).
pub fn pill_cell(st: &Option<Status>) -> Option<(u8, Color)> {
    let m = mobile(st)?;
    if !sim_present(m) {
        return None;
    }
    let on = m.radio_on == Some(true) && registered(m);
    Some(if on {
        (m.signal_bars.unwrap_or(0), text())
    } else {
        (0, text_disabled())
    })
}

/// The modem chip on the screen title.
pub fn modem_chip(st: &Option<Status>) -> (&'static str, Color) {
    match mobile(st).map(|m| m.modem) {
        None => ("Unknown", text_disabled()),
        Some(ModemState::Absent) => ("None", text_disabled()),
        Some(ModemState::Present) => ("Stopped", text_secondary()),
        Some(ModemState::Booting) => ("Starting", warning()),
        Some(ModemState::Ready) => ("Ready", success()),
        Some(ModemState::Failed) => ("Failed", danger()),
    }
}

/// SIM card status: (title, subtitle, subtitle colour).
pub fn sim_lines(st: &Option<Status>) -> (String, String, Color) {
    let Some(m) = mobile(st) else {
        return (
            "Unknown".into(),
            "No status from the system".into(),
            text_disabled(),
        );
    };
    let s = |a: &str, b: &str, c| (a.to_string(), b.to_string(), c);
    match m.modem {
        ModemState::Absent => {
            return s(
                "No modem",
                "Mobile network is not available",
                text_disabled(),
            )
        }
        ModemState::Present => return s("Modem stopped", "The modem is not running", warning()),
        ModemState::Booting => {
            return s(
                "Starting the modem",
                "This takes up to two minutes",
                warning(),
            )
        }
        ModemState::Failed => {
            let why = m.reason.clone().unwrap_or_else(|| "unknown error".into());
            return ("Modem failed".into(), why, danger());
        }
        ModemState::Ready => {}
    }
    match m.sim {
        SimState::Absent => s(
            "No SIM card",
            "Insert a SIM card in slot 1",
            text_secondary(),
        ),
        SimState::PinRequired => s("SIM locked", "Enter the SIM PIN to use it", warning()),
        SimState::PukRequired => s(
            "SIM blocked",
            "PUK required: unlock it in a phone",
            danger(),
        ),
        SimState::Busy => s("SIM card", "Reading the SIM...", warning()),
        SimState::Unknown => s("SIM card", "Checking the SIM...", text_secondary()),
        SimState::Ready if m.radio_on == Some(false) => {
            s("SIM card", "Airplane mode: radio off", text_secondary())
        }
        SimState::Ready => {
            let title = m.operator.clone().unwrap_or_else(|| "SIM card".into());
            let reg = match m.registration {
                MobileReg::Home => "Registered",
                MobileReg::Roaming => "Roaming",
                MobileReg::Searching => "Searching for a network...",
                MobileReg::Denied => "Registration denied",
                MobileReg::NotRegistered => "Not registered",
                MobileReg::Unknown => "Checking the network...",
            };
            let sub = match (&m.tech, registered(m)) {
                (Some(t), true) => format!("{t} · {reg}"),
                _ => reg.to_string(),
            };
            let col = match m.registration {
                MobileReg::Home => success(),
                MobileReg::Roaming | MobileReg::Searching => warning(),
                MobileReg::Denied => danger(),
                _ => text_secondary(),
            };
            (title, sub, col)
        }
    }
}

/// Quick Settings row subtitle.
pub fn qs_line(st: &Option<Status>) -> (String, Color) {
    let Some(m) = mobile(st) else {
        return ("Unknown".into(), text_secondary());
    };
    match (m.modem, m.sim) {
        (ModemState::Absent, _) => ("Not available".into(), text_disabled()),
        (ModemState::Booting, _) => ("Modem starting...".into(), warning()),
        (ModemState::Present, _) => ("Modem stopped".into(), text_secondary()),
        (ModemState::Failed, _) => ("Modem failed".into(), danger()),
        (_, SimState::Absent) => ("No SIM".into(), text_secondary()),
        (_, SimState::PinRequired) => ("SIM PIN required".into(), warning()),
        (_, SimState::PukRequired) => ("SIM blocked (PUK)".into(), danger()),
        _ if m.radio_on == Some(false) => ("Airplane mode".into(), text_secondary()),
        _ if m.data == MobileData::Failed => ("Data failed".into(), danger()),
        _ => {
            let name = m.operator.clone().filter(|_| registered(m));
            match (name, &m.tech) {
                (Some(n), Some(t)) => (format!("{n} · {t}"), text_secondary()),
                (Some(n), None) => (n, text_secondary()),
                _ => ("Searching...".into(), text_secondary()),
            }
        }
    }
}

/// Mobile data line under the switches: (text, colour).
pub fn data_line(st: &Option<Status>) -> (String, Color) {
    let Some(m) = mobile(st) else {
        return ("Unknown".into(), text_disabled());
    };
    match m.data {
        MobileData::Off => ("Mobile data is off".into(), text_secondary()),
        MobileData::Connecting => ("Connecting...".into(), warning()),
        MobileData::On => (
            match &m.ip {
                Some(ip) => format!("Connected, {ip}"),
                None => "Connected".into(),
            },
            success(),
        ),
        MobileData::Failed => (
            format!(
                "Failed: {}",
                m.data_reason.as_deref().unwrap_or("unknown error")
            ),
            danger(),
        ),
    }
}

/// The data switch shows "on" while connected or connecting.
pub fn data_on(st: &Option<Status>) -> bool {
    mobile(st).is_some_and(|m| matches!(m.data, MobileData::On | MobileData::Connecting))
}

/// Data can be switched: modem ready, radio on, SIM unlocked (or data is on: it can go off).
pub fn data_switchable(st: &Option<Status>) -> bool {
    mobile(st).is_some_and(|m| {
        m.modem == ModemState::Ready
            && ((m.radio_on == Some(true) && m.sim == SimState::Ready) || m.data == MobileData::On)
            && m.data != MobileData::Connecting
    })
}

/// The radio switch: on unless the modem says (or was told) off.
pub fn radio_on(st: &Option<Status>) -> bool {
    mobile(st).is_some_and(|m| m.radio_on == Some(true))
}

pub fn radio_switchable(st: &Option<Status>) -> bool {
    mobile(st).is_some_and(|m| m.modem != ModemState::Absent)
}

pub fn pin_needed(st: &Option<Status>) -> bool {
    mobile(st).is_some_and(|m| m.modem == ModemState::Ready && m.sim == SimState::PinRequired)
}

/// IMEI as shown: hidden until "Show" was tapped.
pub fn imei_value(info: &Option<MobileInfo>, shown: bool) -> String {
    match (shown, info.as_ref().and_then(|i| i.imei.as_deref())) {
        (false, _) => "Hidden".into(),
        (true, Some(i)) => i.to_string(),
        (true, None) => match info {
            None => "Loading...".into(),
            Some(_) => "Unknown".into(),
        },
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MobileLayout {
    pub title: Rect,
    pub chip: Rect,
    pub sim_card: Rect,
    pub pin_btn: Option<Rect>,
    pub conn_label: Rect,
    pub radio_row: Rect,
    pub data_row: Rect,
    pub apn_row: Rect,
    pub note: Rect,
    pub modem_label: Rect,
    pub firmware_row: Rect,
    pub imei_row: Rect,
    pub total: i32,
}

pub fn layout(frame: &Frame, st: &Option<Status>) -> MobileLayout {
    let c = frame.padded();
    let mut y = SP_4;
    let chip = Rect::new(c.right() - 120, y + 4, 120, 32);
    let title = Rect::new(c.x, y, chip.x - SP_3 - c.x, TITLE_H);
    y += TITLE_H + SP_4;
    let sim_card = Rect::new(c.x, y, c.w, SIM_CARD_H);
    y += SIM_CARD_H + SP_3;
    let pin_btn = pin_needed(st).then(|| {
        let r = Rect::new(c.x, y, c.w.min(320), BUTTON_H);
        y += BUTTON_H + SP_3;
        r
    });
    y += SP_3;
    let conn_label = Rect::new(c.x, y, c.w, LABEL_H);
    y += LABEL_H + SP_2;
    let radio_row = Rect::new(c.x, y, c.w, ROW_H);
    let data_row = Rect::new(c.x, y + ROW_H, c.w, ROW_H);
    let apn_row = Rect::new(c.x, y + 2 * ROW_H, c.w, ROW_H);
    y += 3 * ROW_H + SP_3;
    let note = Rect::new(c.x + SP_1, y, c.w - SP_1, NOTE_H);
    y += NOTE_H + SP_4;
    let modem_label = Rect::new(c.x, y, c.w, LABEL_H);
    y += LABEL_H + SP_2;
    let firmware_row = Rect::new(c.x, y, c.w, ROW_H);
    let imei_row = Rect::new(c.x, y + ROW_H, c.w, ROW_H);
    y += 2 * ROW_H;
    MobileLayout {
        title,
        chip,
        sim_card,
        pin_btn,
        conn_label,
        radio_row,
        data_row,
        apn_row,
        note,
        modem_label,
        firmware_row,
        imei_row,
        total: y + SP_6,
    }
}

/// Everything the screen draws from.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn st(f: impl FnOnce(&mut MobileStatus)) -> Option<Status> {
        let mut s = crate::app::tests::status(120, &[]);
        f(&mut s.mobile);
        Some(s)
    }
    fn registered_gp(m: &mut MobileStatus) {
        m.modem = ModemState::Ready;
        m.radio_on = Some(true);
        m.sim = SimState::Ready;
        m.registration = MobileReg::Home;
        m.operator = Some("Grameenphone".into());
        m.tech = Some("4G".into());
        m.signal_bars = Some(3);
        m.signal_dbm = Some(-71);
    }

    #[test]
    fn lines_for_every_state() {
        assert_eq!(qs_line(&None).0, "Unknown");
        assert_eq!(qs_line(&st(|_| {})).0, "Not available");
        assert_eq!(
            qs_line(&st(|m| m.modem = ModemState::Booting)).0,
            "Modem starting..."
        );
        let gp = st(registered_gp);
        assert_eq!(qs_line(&gp).0, "Grameenphone · 4G");
        assert_eq!(sim_lines(&gp).0, "Grameenphone");
        assert_eq!(sim_lines(&gp).1, "4G · Registered");
        assert_eq!(pill_cell(&gp), Some((3, TEXT)));
        let pin = st(|m| {
            m.modem = ModemState::Ready;
            m.sim = SimState::PinRequired;
        });
        assert_eq!(qs_line(&pin).0, "SIM PIN required");
        assert!(pin_needed(&pin));
        assert_eq!(pill_cell(&pin), Some((0, TEXT_DISABLED)));
        let nosim = st(|m| {
            m.modem = ModemState::Ready;
            m.sim = SimState::Absent;
        });
        assert_eq!(pill_cell(&nosim), None, "no SIM: no bars in the pill");
        assert_eq!(sim_lines(&nosim).0, "No SIM card");
        let off = st(|m| {
            registered_gp(m);
            m.radio_on = Some(false);
        });
        assert_eq!(qs_line(&off).0, "Airplane mode");
        assert!(!data_switchable(&off));
        let failed = st(|m| {
            registered_gp(m);
            m.data = MobileData::Failed;
            m.data_reason = Some("data link not supported yet".into());
        });
        assert_eq!(data_line(&failed).0, "Failed: data link not supported yet");
        assert!(data_switchable(&failed) && !data_on(&failed));
        let mfail = st(|m| {
            m.modem = ModemState::Failed;
            m.reason = Some("the modem did not start".into());
        });
        assert_eq!(sim_lines(&mfail).1, "the modem did not start");
    }

    #[test]
    fn imei_is_hidden_until_asked() {
        let info = Some(MobileInfo {
            imei: Some("351234567890123".into()),
            iccid: None,
        });
        assert_eq!(imei_value(&info, false), "Hidden");
        assert_eq!(imei_value(&info, true), "351234567890123");
        assert_eq!(imei_value(&None, true), "Loading...");
    }

    #[test]
    fn layout_rows_are_touchable_and_do_not_overlap() {
        for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
            for s in [
                st(registered_gp),
                st(|m| {
                    m.modem = ModemState::Ready;
                    m.sim = SimState::PinRequired;
                }),
            ] {
                let l = layout(&f, &s);
                let mut rs = vec![
                    l.title,
                    l.chip,
                    l.sim_card,
                    l.conn_label,
                    l.radio_row,
                    l.data_row,
                    l.apn_row,
                    l.note,
                    l.modem_label,
                    l.firmware_row,
                    l.imei_row,
                ];
                rs.extend(l.pin_btn);
                for r in [l.radio_row, l.data_row, l.apn_row, l.imei_row] {
                    assert!(r.h >= TOUCH_MIN);
                }
                for (i, a) in rs.iter().enumerate() {
                    for b in &rs[i + 1..] {
                        let o = a.x < b.right()
                            && b.x < a.right()
                            && a.y < b.bottom()
                            && b.y < a.bottom();
                        assert!(!o, "{a:?} overlaps {b:?}");
                    }
                }
                assert!(l.total > l.imei_row.bottom());
            }
        }
        assert!(layout(&Frame::new(800, 1280), &st(registered_gp))
            .pin_btn
            .is_none());
    }
}
