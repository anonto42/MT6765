//! Network screen: model, layout (pure) and rendering. Content coordinates: x is absolute, y starts
//! at 0 at the top of the scrollable content.
use crate::app::Target;
use walpad_proto::{HotspotInternet, HotspotState, Status, WifiRadio};
use walpad_ui::components::{Accessory, ButtonKind, ButtonState, NetState, Painter};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Color, Face, Icon, Rect, Style};

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

const TITLE_H: i32 = 40;
const CARD_H: i32 = 112;
const LABEL_H: i32 = 24;
const FORGET_W: i32 = 120;
const SWITCH_W: i32 = 56;
/// Hotspot status lines under its card (up to three 22 px lines).
const NOTE_H: i32 = 72;

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
        return vec![("Unknown".into(), TEXT_DISABLED)];
    };
    let h = &s.hotspot;
    let mut v = Vec::new();
    match h.state {
        HotspotState::Off => {
            v.push(("Off".to_string(), TEXT_SECONDARY));
            if !h.has_secret {
                v.push(("Set a password to use the hotspot".into(), WARNING));
            }
        }
        HotspotState::Starting => v.push(("Starting...".into(), WARNING)),
        HotspotState::Stopping => v.push(("Stopping...".into(), TEXT_SECONDARY)),
        HotspotState::Failed => v.push((
            format!("Failed: {}", h.reason.as_deref().unwrap_or("unknown error")),
            DANGER,
        )),
        HotspotState::On => {
            let n = h.clients.unwrap_or(0);
            let devices = if n == 1 { "device" } else { "devices" };
            v.push((format!("On, {n} {devices} connected"), SUCCESS));
            v.push(match h.internet {
                Some(HotspotInternet::Shared) => (
                    "Sharing this tablet's internet connection".into(),
                    TEXT_SECONDARY,
                ),
                Some(HotspotInternet::Unavailable) => (
                    "Internet sharing is not available: local network only".into(),
                    WARNING,
                ),
                _ => (
                    "No internet to share: local network only".into(),
                    TEXT_SECONDARY,
                ),
            });
            if h.exclusive {
                v.push((
                    "Wi-Fi client paused while the hotspot is on".into(),
                    WARNING,
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

/// Draws the screen into `p` (an off-screen canvas as tall as `layout.total`) and returns the hit
/// rectangles in content coordinates.
pub fn render(
    p: &mut Painter,
    frame: &Frame,
    st: &Option<Status>,
    net: &NetModel,
    connected_core: bool,
    pressed: Option<Target>,
    spinner_deg: u16,
) -> Vec<(Rect, Target)> {
    let l = layout(frame, st, net);
    let mut hits = Vec::new();
    let pr = |t: Target| pressed == Some(t);
    p.screen_title("Wi-Fi", l.title);

    // radio chip and the Wi-Fi switch
    let enabled = wifi_enabled(st);
    let paused = st
        .as_ref()
        .is_some_and(|s| s.wifi.state.as_deref() == Some("hotspot"));
    let (chip_text, chip_col) = match st.as_ref().map(|s| s.wifi.radio) {
        Some(WifiRadio::Failed) => ("Failed", DANGER),
        Some(_) if !enabled => ("Off", TEXT_SECONDARY),
        Some(_) if paused => ("Paused", WARNING),
        Some(WifiRadio::Up) => ("Up", SUCCESS),
        Some(WifiRadio::Starting) => ("Starting", WARNING),
        Some(WifiRadio::Down) => ("Down", TEXT_SECONDARY),
        None => ("Unknown", TEXT_DISABLED),
    };
    let sw_off = !connected_core || st.is_none();
    p.toggle(
        l.switch.x,
        l.switch.y,
        enabled,
        pr(Target::WifiSwitch),
        sw_off,
    );
    hits.push((switch_hit(l.switch), Target::WifiSwitch));
    p.outline_box(l.chip, 0, false, false, false);
    p.cv.rect(Rect::new(l.chip.x, l.chip.y, 3, l.chip.h), chip_col);
    p.cv.rect(Rect::new(l.chip.x + 12, l.chip.y + 12, 8, 8), chip_col);
    p.text(
        chip_text,
        Style::new(Face::Regular, SIZE_SECONDARY, TEXT),
        Rect::new(l.chip.x + 28, l.chip.y, l.chip.w - 36, l.chip.h),
        Align::Left,
    );
    if let Some(h) = l.hint {
        p.text(
            "Restart the tablet to retry Wi-Fi",
            Style::new(Face::Regular, SIZE_SECONDARY, DANGER),
            h,
            Align::Left,
        );
    }

    // connected card
    if let (Some((card, btn)), Some(s)) = (l.connected, st.as_ref()) {
        p.card(card, false);
        let ssid = s.wifi.ssid.clone().unwrap_or_default();
        p.text(
            &ssid,
            Style::new(Face::Bold, SIZE_BODY, TEXT),
            Rect::new(card.x + SP_4, card.y + SP_4, card.w - 2 * SP_4 - 170, 28),
            Align::Left,
        );
        p.signal_bars(
            card.x + SP_4,
            card.y + 52,
            s.wifi.signal_dbm.map_or(0, bars_from_dbm),
            SUCCESS,
        );
        if let Some(ip) = &s.wifi.ip {
            p.text(
                ip,
                Style::new(Face::Mono, SIZE_MONO, TEXT_SECONDARY),
                Rect::new(card.x + SP_4 + 40, card.y + 46, 300, 28),
                Align::Left,
            );
        }
        p.text(
            "Connected",
            Style::new(Face::Regular, SIZE_SECONDARY, SUCCESS),
            Rect::new(card.x + SP_4, card.y + 78, 200, 24),
            Align::Left,
        );
        let state = if !connected_core {
            ButtonState::Disabled
        } else if pr(Target::Disconnect) {
            ButtonState::Pressed
        } else {
            ButtonState::Default
        };
        p.button(btn, "Disconnect", ButtonKind::Secondary, state, None);
        hits.push((btn, Target::Disconnect));
    }

    // available networks
    p.section_label("Available networks", l.avail_label);
    let client_off = client_off(st);
    let scan_state = if net.scanning {
        ButtonState::Loading(spinner_deg)
    } else if !connected_core || client_off {
        ButtonState::Disabled
    } else if pr(Target::Scan) {
        ButtonState::Pressed
    } else {
        ButtonState::Default
    };
    p.button(
        l.scan_btn,
        if net.scanning { "Scanning" } else { "Scan" },
        ButtonKind::Secondary,
        scan_state,
        Some(Icon::Refresh),
    );
    hits.push((l.scan_btn, Target::Scan));
    p.card(l.avail_card, false);
    if let Some(r) = l.avail_empty {
        let (title, hint) = match (&net.error, net.scanned, net.scanning) {
            _ if client_off && !enabled => ("Wi-Fi is off", "Turn Wi-Fi on to see networks"),
            _ if client_off => ("Wi-Fi is paused", "The hotspot is using the radio"),
            (Some(e), _, _) => ("Cannot scan", e.as_str()),
            (None, _, true) => ("Looking for networks", "This takes a few seconds"),
            (None, true, false) => ("No networks found", "Tap Scan to look again"),
            (None, false, false) => ("No scan yet", "Tap Scan to look for networks"),
        };
        p.empty_state(r, Icon::Wifi, title, hint);
    }
    let connected_ssid = is_connected(st);
    for (i, (r, n)) in l.avail_rows.iter().zip(&net.results).enumerate() {
        let state = if Some(n.ssid.as_str()) == connected_ssid {
            NetState::Connected
        } else if net.connecting.as_deref() == Some(n.ssid_hex.as_str()) {
            NetState::Connecting(spinner_deg)
        } else {
            NetState::Idle
        };
        p.network_row(*r, &n.ssid, n.bars, n.secured, state, pr(Target::Avail(i)));
        hits.push((*r, Target::Avail(i)));
    }

    // saved networks
    p.section_label("Saved networks", l.saved_label);
    p.card(l.saved_card, false);
    if let Some(r) = l.saved_empty {
        p.text(
            "No saved networks",
            Style::new(Face::Regular, SIZE_BODY, TEXT_DISABLED),
            Rect::new(r.x + SP_4, r.y, r.w - 2 * SP_4, r.h),
            Align::Left,
        );
    }
    for (i, ((r, forget), s)) in l.saved_rows.iter().zip(&net.saved).enumerate() {
        let sub = (Some(s.ssid.as_str()) == connected_ssid).then_some("Connected");
        p.list_row(
            *r,
            None,
            &s.ssid,
            sub,
            &Accessory::None,
            pr(Target::Saved(i)),
            false,
        );
        p.text(
            "Forget",
            Style::new(Face::Bold, SIZE_BODY, DANGER),
            Rect::new(forget.x, forget.y, forget.w - SP_4, forget.h),
            Align::Right,
        );
        hits.push((*r, Target::Saved(i)));
        hits.push((*forget, Target::Forget(i))); // later entries win: the Forget zone sits on top of the row
    }

    // hotspot (owner request 2026-11-10)
    p.section_label("Hotspot", l.hotspot_label);
    let hs = st.as_ref().map(|s| &s.hotspot);
    let busy =
        hs.is_some_and(|h| matches!(h.state, HotspotState::Starting | HotspotState::Stopping));
    p.toggle(
        l.hotspot_switch.x,
        l.hotspot_switch.y,
        hotspot_on(st),
        pr(Target::HotspotSwitch),
        sw_off || busy,
    );
    hits.push((switch_hit(l.hotspot_switch), Target::HotspotSwitch));
    p.card(l.hotspot_card, false);
    let rows_off = sw_off || busy;
    let name = hs
        .and_then(|h| h.ssid.clone())
        .unwrap_or_else(|| "WalPad".into());
    let pass = if hs.is_some_and(|h| h.has_secret) {
        "Set"
    } else {
        "Not set"
    };
    for (r, title, value, t) in [
        (l.hotspot_name, "Name", name, Target::HotspotName),
        (
            l.hotspot_pass,
            "Password",
            pass.to_string(),
            Target::HotspotPassword,
        ),
    ] {
        p.list_row(
            r,
            None,
            title,
            None,
            &Accessory::Value(value),
            pr(t),
            rows_off,
        );
        hits.push((r, t));
    }
    for (i, (text, col)) in hotspot_lines(st).iter().take(3).enumerate() {
        p.text(
            text,
            Style::new(Face::Regular, SIZE_SECONDARY, *col),
            Rect::new(
                l.hotspot_note.x,
                l.hotspot_note.y + i as i32 * 24,
                l.hotspot_note.w,
                22,
            ),
            Align::Left,
        );
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlaps(a: Rect, b: Rect) -> bool {
        a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
    }

    fn net(results: usize, saved: usize) -> NetModel {
        NetModel {
            results: (0..results)
                .map(|i| NetEntry {
                    ssid_hex: format!("{i:02x}"),
                    ssid: format!("Net {i}"),
                    bars: 3,
                    secured: i % 2 == 0,
                })
                .collect(),
            saved: (0..saved)
                .map(|i| SavedEntry {
                    id: i as u32,
                    ssid: format!("Saved {i}"),
                })
                .collect(),
            ..NetModel::default()
        }
    }

    #[test]
    fn signal_steps() {
        assert_eq!(
            [-40, -55, -56, -65, -70, -75, -80, -95].map(bars_from_dbm),
            [4, 4, 3, 3, 2, 2, 1, 1]
        );
    }

    #[test]
    fn layout_has_no_overlaps_and_rows_are_big_enough() {
        for (frame, n) in [
            (Frame::new(800, 1280), net(7, 3)),
            (Frame::new(1280, 800), net(0, 0)),
            (Frame::new(800, 1280), net(1, 0)),
        ] {
            let l = layout(&frame, &None, &n);
            let mut rs = vec![
                l.title,
                l.avail_label,
                l.scan_btn,
                l.avail_card,
                l.saved_label,
                l.saved_card,
                l.chip,
                l.switch,
                l.hotspot_label,
                l.hotspot_switch,
                l.hotspot_card,
                l.hotspot_note,
            ];
            rs.retain(|r| r.h > 0);
            // the scan button sits on the label row, to the right of the label
            for (i, a) in rs.iter().enumerate() {
                for b in &rs[i + 1..] {
                    if (*a == l.scan_btn && *b == l.avail_label)
                        || (*b == l.scan_btn && *a == l.avail_label)
                    {
                        continue;
                    }
                    assert!(!overlaps(*a, *b), "{a:?} overlaps {b:?}");
                }
            }
            assert!(
                l.avail_rows.iter().all(|r| r.h >= ROW_WIFI_H)
                    && l.saved_rows
                        .iter()
                        .all(|(r, f)| r.h >= ROW_H && f.w >= TOUCH_MIN && r.contains(f.x, f.y))
            );
            assert!(l.scan_btn.h >= TOUCH_MIN);
            assert!(l.total > l.saved_card.bottom());
            assert!(l.total >= l.hotspot_note.bottom());
            for s in [l.switch, l.hotspot_switch] {
                let h = switch_hit(s);
                assert!(h.w >= TOUCH_MIN && h.h >= TOUCH_MIN && h.contains(s.x, s.y));
                assert!(h.right() <= frame.content.right(), "{h:?}");
            }
            assert!(l.hotspot_name.h >= ROW_H && l.hotspot_pass.y == l.hotspot_name.bottom());
        }
    }

    #[test]
    fn many_networks_make_the_content_scroll() {
        let f = Frame::new(800, 1280);
        let short = layout(&f, &None, &net(1, 0)).total;
        let long = layout(&f, &None, &net(12, 4)).total;
        assert!(
            long > f.content.h,
            "12 networks do not fit one screen ({long})"
        );
        assert!(long > short);
        assert_eq!(layout(&f, &None, &net(12, 0)).avail_rows.len(), 12);
    }

    #[test]
    fn connected_card_and_failed_hint_take_space_only_when_present() {
        let f = Frame::new(800, 1280);
        let base = layout(&f, &None, &net(2, 1));
        assert!(base.connected.is_none() && base.hint.is_none());
        let mut st = crate::app::tests::status(120, &[]);
        let with = layout(&f, &Some(st.clone()), &net(2, 1));
        assert!(with.connected.is_some());
        assert_eq!(with.total - base.total, CARD_H + SP_5);
        st.wifi.radio = WifiRadio::Failed;
        st.wifi.ssid = None;
        assert!(layout(&f, &Some(st), &net(2, 1)).hint.is_some());
    }

    #[test]
    fn switched_off_wifi_shows_no_connection() {
        let mut st = crate::app::tests::status(120, &[]);
        assert_eq!(is_connected(&Some(st.clone())), Some("Home"));
        st.wifi.enabled = false;
        assert_eq!(is_connected(&Some(st.clone())), None);
        assert!(!wifi_enabled(&Some(st)));
        assert!(!wifi_enabled(&None));
    }

    #[test]
    fn hotspot_status_lines() {
        use walpad_proto::HotspotStatus;
        let mut st = crate::app::tests::status(120, &[]);
        let lines = |s: &Status| -> Vec<String> {
            hotspot_lines(&Some(s.clone()))
                .into_iter()
                .map(|(t, _)| t)
                .collect()
        };
        assert_eq!(lines(&st), ["Off", "Set a password to use the hotspot"]);
        st.hotspot = HotspotStatus {
            state: HotspotState::On,
            has_secret: true,
            clients: Some(1),
            internet: Some(HotspotInternet::Unavailable),
            exclusive: true,
            ..Default::default()
        };
        assert!(hotspot_on(&Some(st.clone())));
        assert_eq!(
            lines(&st),
            [
                "On, 1 device connected",
                "Internet sharing is not available: local network only",
                "Wi-Fi client paused while the hotspot is on"
            ]
        );
        st.hotspot.state = HotspotState::Failed;
        st.hotspot.reason = Some("hostapd is not installed".into());
        assert!(!hotspot_on(&Some(st.clone())));
        assert_eq!(lines(&st), ["Failed: hostapd is not installed"]);
    }
}
