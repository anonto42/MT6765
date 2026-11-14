//! Network screen: model, layout (pure) and rendering. Content coordinates: x is absolute, y starts
//! at 0 at the top of the scrollable content.
use crate::app::Target;
use walpad_proto::{HotspotState, Status, WifiRadio};
use walpad_ui::components::{Accessory, ButtonKind, ButtonState, NetState, Painter};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Icon, Rect, Style};

mod model;
#[cfg(test)]
mod tests;

pub use model::{
    bars_from_dbm, client_off, hotspot_lines, hotspot_on, is_connected, layout, switch_hit,
    wifi_enabled, NetEntry, NetLayout, NetModel, SavedEntry,
};
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
        Some(WifiRadio::Failed) => ("Failed", danger()),
        Some(_) if !enabled => ("Off", text_secondary()),
        Some(_) if paused => ("Paused", warning()),
        Some(WifiRadio::Up) => ("Up", success()),
        Some(WifiRadio::Starting) => ("Starting", warning()),
        Some(WifiRadio::Down) => ("Down", text_secondary()),
        None => ("Unknown", text_disabled()),
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
        Style::new(Face::Regular, SIZE_SECONDARY, text()),
        Rect::new(l.chip.x + 28, l.chip.y, l.chip.w - 36, l.chip.h),
        Align::Left,
    );
    if let Some(h) = l.hint {
        p.text(
            "Restart the tablet to retry Wi-Fi",
            Style::new(Face::Regular, SIZE_SECONDARY, danger()),
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
            Style::new(Face::Bold, SIZE_BODY, text()),
            Rect::new(card.x + SP_4, card.y + SP_4, card.w - 2 * SP_4 - 170, 28),
            Align::Left,
        );
        p.signal_bars(
            card.x + SP_4,
            card.y + 52,
            s.wifi.signal_dbm.map_or(0, bars_from_dbm),
            success(),
        );
        if let Some(ip) = &s.wifi.ip {
            p.text(
                ip,
                Style::new(Face::Mono, SIZE_MONO, text_secondary()),
                Rect::new(card.x + SP_4 + 40, card.y + 46, 300, 28),
                Align::Left,
            );
        }
        p.text(
            "Connected",
            Style::new(Face::Regular, SIZE_SECONDARY, success()),
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
            Style::new(Face::Regular, SIZE_BODY, text_disabled()),
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
            Style::new(Face::Bold, SIZE_BODY, danger()),
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
