//! Home: the app grid (every installed app plus the Settings shortcut) as outlined 64 px icon
//! boxes with the name underneath, centred in the main area over the watermark. Fits without
//! scrolling; the clock and status live in the header. Screen coordinates.
use crate::app::{Model, Target};
use walpad_proto::{BatterySource, WifiRadio};
use walpad_ui::components::{CardValue, Painter};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::Rect;

/// Grid cell: the 64 px box, a gap and the label line.
pub const CELL_W: i32 = 120;
pub const CELL_H: i32 = BOX_GRID + 6 + 18;
const ROW_GAP: i32 = SP_6;
const TOP: i32 = 40;

#[derive(Debug, Clone, PartialEq)]
pub struct HomeLayout {
    pub tiles: Vec<Rect>,
    pub bottom: i32,
}

/// `banner_h` is the height reserved at the top of the content for a persistent banner.
pub fn layout(frame: &Frame, banner_h: i32, tile_count: usize) -> HomeLayout {
    let c = frame.padded();
    let cols = (c.w / CELL_W).clamp(1, (tile_count as i32).max(1));
    let x0 = c.x + (c.w - cols * CELL_W) / 2;
    let y0 = c.y + banner_h + TOP;
    let tiles: Vec<Rect> = (0..tile_count as i32)
        .map(|i| {
            Rect::new(
                x0 + (i % cols) * CELL_W,
                y0 + (i / cols) * (CELL_H + ROW_GAP),
                CELL_W,
                CELL_H,
            )
        })
        .collect();
    HomeLayout {
        bottom: tiles.last().map_or(y0, |r| r.bottom()),
        tiles,
    }
}

pub fn battery_card(m: &Model) -> (String, CardValue) {
    let Some(st) = &m.status else {
        return ("Battery".into(), CardValue::Unknown);
    };
    let b = &st.battery;
    let state = b.state.as_deref().unwrap_or("").to_ascii_lowercase();
    let title = if state.starts_with("charg") {
        "Battery · Charging"
    } else if state.starts_with("full") {
        "Battery · Full"
    } else {
        "Battery"
    };
    let value = match (b.percent, b.source) {
        (Some(p), BatterySource::Gauge) => CardValue::Value(format!("{p}%")),
        (Some(p), BatterySource::VoltageEstimate) => CardValue::Estimated(format!("{p}%")),
        _ => CardValue::Unknown,
    };
    (title.into(), value)
}

/// What to call the current Wi-Fi link: the SSID when the supplicant told us, "Connected" when we
/// only know there is an address, otherwise None.
pub fn wifi_name(st: &walpad_proto::Status) -> Option<String> {
    match (&st.wifi.ssid, &st.wifi.ip) {
        (Some(s), _) => Some(s.clone()),
        (None, Some(_)) => Some("Connected".into()),
        (None, None) => None,
    }
}

pub fn wifi_card(m: &Model) -> (CardValue, Option<String>) {
    let Some(st) = &m.status else {
        return (CardValue::Unknown, None);
    };
    let w = &st.wifi;
    if w.radio == WifiRadio::Failed {
        return (
            CardValue::Error("Wi-Fi failed".into()),
            Some("Restart the tablet to retry".into()),
        );
    }
    match wifi_name(st) {
        Some(n) => (CardValue::Value(n), w.ip.clone()),
        None => (CardValue::Value("Not connected".into()), None),
    }
}

/// Draws Home and returns the hit rectangles (`AppTile(i)` indexes `model.apps`).
pub fn render(p: &mut Painter, frame: &Frame, m: &Model, banner_h: i32) -> Vec<(Rect, Target)> {
    let l = layout(frame, banner_h, m.apps.len());
    // the Apps panel has no fill: grid items under it are not drawn while it is open
    let panel = matches!(m.overlay, Some(crate::overlay::Overlay::AppsPanel))
        .then(|| crate::panels::apps_panel_rect(frame, m).inset(-SP_2));
    let mut hits = Vec::new();
    for (i, (r, t)) in l.tiles.iter().zip(&m.apps).enumerate() {
        let target = Target::AppTile(i);
        if panel.is_some_and(|pr| {
            r.x < pr.right() && pr.x < r.right() && r.y < pr.bottom() && pr.y < r.bottom()
        }) {
            continue;
        }
        p.app_icon(
            *r,
            BOX_GRID,
            t.icon,
            &t.name,
            m.pressed == Some(target),
            t.running,
        );
        if t.id == "phone" {
            // the unread text messages (owner request 2026-11-11)
            let b = Rect::new(r.x + (r.w - BOX_GRID) / 2, r.y, BOX_GRID, BOX_GRID);
            crate::phone_ui::paint_badge(p, b, crate::phone_ui::unread(&m.status));
        }
        hits.push((*r, target));
    }
    hits
}

#[cfg(test)]
mod tests;
