use walpad_proto::{MobileData, MobileReg, MobileStatus, ModemState, SimState, Status};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Color, Rect};

pub(super) const TITLE_H: i32 = 40;
pub(super) const SIM_CARD_H: i32 = 112;
pub(super) const LABEL_H: i32 = 24;
pub(super) const NOTE_H: i32 = 52;

/// The identifiers `mobile_info` returned (shown on request only, never logged).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MobileInfo {
    pub imei: Option<String>,
    pub iccid: Option<String>,
}

pub(super) fn mobile(st: &Option<Status>) -> Option<&MobileStatus> {
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

pub(super) fn registered(m: &MobileStatus) -> bool {
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
