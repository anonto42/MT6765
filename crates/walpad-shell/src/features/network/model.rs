use walpad_proto::{HotspotInternet, HotspotState, Status, WifiRadio};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Color, Rect};

#[derive(Debug, Clone, PartialEq)]
pub struct NetEntry {
    pub ssid_hex: String,
    pub ssid: String,
    pub bars: u8,
    pub secured: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SavedEntry {
    pub id: u32,
    pub ssid: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NetModel {
    pub scanning: bool,
    pub scanned: bool,
    pub results: Vec<NetEntry>,
    pub saved: Vec<SavedEntry>,
    /// SSID (hex) of the connection attempt in progress.
    pub connecting: Option<String>,
    /// Last scan / control error ("Wi-Fi control is unavailable ...").
    pub error: Option<String>,
}

/// Four-step signal from dBm.
pub fn bars_from_dbm(dbm: i32) -> u8 {
    match dbm {
        d if d >= -55 => 4,
        d if d >= -65 => 3,
        d if d >= -75 => 2,
        _ => 1,
    }
}

pub(super) const TITLE_H: i32 = 40;
pub(super) const CARD_H: i32 = 112;
pub(super) const LABEL_H: i32 = 24;
pub(super) const FORGET_W: i32 = 120;
pub(super) const SWITCH_W: i32 = 56;
/// Hotspot status lines under its card (up to three 22 px lines).
pub(super) const NOTE_H: i32 = 72;

/// Touch area around a 56x32 toggle (>= 56 high).
pub fn switch_hit(t: Rect) -> Rect {
    Rect::new(t.x - SP_2, t.y - 12, t.w + SP_4, t.h + 24)
}

/// The Wi-Fi client switch as the core reports it (unknown = off).
pub fn wifi_enabled(st: &Option<Status>) -> bool {
    st.as_ref().is_some_and(|s| s.wifi.enabled)
}

/// The Wi-Fi client is switched off or paused for the hotspot (known status only).
pub fn client_off(st: &Option<Status>) -> bool {
    st.as_ref()
        .is_some_and(|s| !s.wifi.enabled || s.wifi.state.as_deref() == Some("hotspot"))
}

/// The hotspot switch shows "on" while it runs or starts.
pub fn hotspot_on(st: &Option<Status>) -> bool {
    st.as_ref()
        .is_some_and(|s| matches!(s.hotspot.state, HotspotState::On | HotspotState::Starting))
}

/// Status lines of the hotspot (text, colour), most important first.
pub fn hotspot_lines(st: &Option<Status>) -> Vec<(String, Color)> {
    let Some(s) = st else {
        return vec![("Unknown".into(), text_disabled())];
    };
    let h = &s.hotspot;
    let mut v = Vec::new();
    match h.state {
        HotspotState::Off => {
            v.push(("Off".to_string(), text_secondary()));
            if !h.has_secret {
                v.push(("Set a password to use the hotspot".into(), warning()));
            }
        }
        HotspotState::Starting => v.push(("Starting...".into(), warning())),
        HotspotState::Stopping => v.push(("Stopping...".into(), text_secondary())),
        HotspotState::Failed => v.push((
            format!("Failed: {}", h.reason.as_deref().unwrap_or("unknown error")),
            danger(),
        )),
        HotspotState::On => {
            let n = h.clients.unwrap_or(0);
            let devices = if n == 1 { "device" } else { "devices" };
            v.push((format!("On, {n} {devices} connected"), success()));
            v.push(match h.internet {
                Some(HotspotInternet::Shared) => (
                    "Sharing this tablet's internet connection".into(),
                    text_secondary(),
                ),
                Some(HotspotInternet::Unavailable) => (
                    "Internet sharing is not available: local network only".into(),
                    warning(),
                ),
                _ => (
                    "No internet to share: local network only".into(),
                    text_secondary(),
                ),
            });
            if h.exclusive {
                v.push((
                    "Wi-Fi client paused while the hotspot is on".into(),
                    warning(),
                ));
            }
        }
    }
    v
}

#[derive(Debug, Clone, PartialEq)]
pub struct NetLayout {
    pub title: Rect,
    pub chip: Rect,
    /// The Wi-Fi client switch (the toggle itself; hit area via `switch_hit`).
    pub switch: Rect,
    pub hint: Option<Rect>,
    pub connected: Option<(Rect, Rect)>,
    pub avail_label: Rect,
    pub scan_btn: Rect,
    pub avail_card: Rect,
    pub avail_rows: Vec<Rect>,
    pub avail_empty: Option<Rect>,
    pub saved_label: Rect,
    pub saved_card: Rect,
    pub saved_rows: Vec<(Rect, Rect)>,
    pub saved_empty: Option<Rect>,
    pub hotspot_label: Rect,
    pub hotspot_switch: Rect,
    pub hotspot_card: Rect,
    pub hotspot_name: Rect,
    pub hotspot_pass: Rect,
    pub hotspot_note: Rect,
    pub total: i32,
}

pub fn is_connected(st: &Option<Status>) -> Option<&str> {
    st.as_ref()
        .filter(|s| s.wifi.enabled)
        .and_then(|s| s.wifi.ssid.as_deref())
}

pub fn layout(frame: &Frame, st: &Option<Status>, net: &NetModel) -> NetLayout {
    let c = frame.padded();
    let mut y = SP_4;
    let switch = Rect::new(c.right() - SWITCH_W, y + 4, SWITCH_W, 32);
    let chip = Rect::new(switch.x - SP_5 - 120, y + 4, 120, 32);
    let title = Rect::new(c.x, y, chip.x - SP_3 - c.x, TITLE_H);
    y += TITLE_H + SP_2;
    let failed = st
        .as_ref()
        .is_some_and(|s| s.wifi.radio == WifiRadio::Failed);
    let hint = failed.then(|| {
        let r = Rect::new(c.x, y, c.w, 24);
        y += 24 + SP_2;
        r
    });
    y += SP_2;
    let connected = is_connected(st).map(|_| {
        let card = Rect::new(c.x, y, c.w, CARD_H);
        let btn = Rect::new(
            card.right() - SP_4 - 150,
            card.y + (CARD_H - BUTTON_H) / 2,
            150,
            BUTTON_H,
        );
        y += CARD_H + SP_5;
        (card, btn)
    });
    let avail_label = Rect::new(c.x, y + (BUTTON_H - LABEL_H) / 2, c.w - 168, LABEL_H);
    let scan_btn = Rect::new(c.right() - 152, y, 152, BUTTON_H);
    y += BUTTON_H + SP_2;
    let rows_h = net.results.len() as i32 * ROW_WIFI_H;
    let (avail_card, avail_rows, avail_empty);
    if net.results.is_empty() {
        let r = Rect::new(c.x, y, c.w, 160);
        avail_card = r;
        avail_rows = vec![];
        avail_empty = Some(r);
        y += 160 + SP_5;
    } else {
        avail_card = Rect::new(c.x, y, c.w, rows_h);
        avail_rows = (0..net.results.len() as i32)
            .map(|i| Rect::new(c.x, y + i * ROW_WIFI_H, c.w, ROW_WIFI_H))
            .collect();
        avail_empty = None;
        y += rows_h + SP_5;
    }
    let saved_label = Rect::new(c.x, y, c.w, LABEL_H);
    y += LABEL_H + SP_2;
    let (saved_card, saved_rows, saved_empty);
    if net.saved.is_empty() {
        let r = Rect::new(c.x, y, c.w, 56);
        saved_card = r;
        saved_rows = vec![];
        saved_empty = Some(r);
        y += 56;
    } else {
        let h = net.saved.len() as i32 * ROW_H;
        saved_card = Rect::new(c.x, y, c.w, h);
        saved_rows = (0..net.saved.len() as i32)
            .map(|i| {
                let r = Rect::new(c.x, y + i * ROW_H, c.w, ROW_H);
                (r, Rect::new(r.right() - FORGET_W, r.y, FORGET_W, r.h))
            })
            .collect();
        saved_empty = None;
        y += h;
    }
    y += SP_5;
    let hotspot_label = Rect::new(c.x, y, c.w - SWITCH_W - SP_4, LABEL_H);
    let hotspot_switch = Rect::new(c.right() - SWITCH_W, y - 4, SWITCH_W, 32);
    y += LABEL_H + SP_3;
    let hotspot_card = Rect::new(c.x, y, c.w, 2 * ROW_H);
    let hotspot_name = Rect::new(c.x, y, c.w, ROW_H);
    let hotspot_pass = Rect::new(c.x, y + ROW_H, c.w, ROW_H);
    y += 2 * ROW_H + SP_3;
    let hotspot_note = Rect::new(c.x + SP_1, y, c.w - SP_1, NOTE_H);
    y += NOTE_H;
    NetLayout {
        title,
        chip,
        switch,
        hotspot_label,
        hotspot_switch,
        hotspot_card,
        hotspot_name,
        hotspot_pass,
        hotspot_note,
        hint,
        connected,
        avail_label,
        scan_btn,
        avail_card,
        avail_rows,
        avail_empty,
        saved_label,
        saved_card,
        saved_rows,
        saved_empty,
        total: y + SP_6,
    }
}
