use crate::app::Model;
use crate::home::wifi_name;
use walpad_proto::{BatterySource, HotspotState, WifiRadio};
use walpad_ui::components::WifiMark;
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Color, Rect};

pub const QS_W: i32 = 280;
pub(super) const QS_PAD: i32 = 8;
pub(super) const QS_ROW: i32 = 56;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QsGeom {
    pub panel: Rect,
    pub wifi: Rect,
    pub hotspot: Rect,
    /// Mobile data (owner request 2026-11-10).
    pub mobile: Rect,
    pub bluetooth: Rect,
    /// GPS (owner request 2026-11-11).
    pub gps: Rect,
    pub battery: Rect,
    pub divider1: i32,
    pub brightness_label: Rect,
    pub slider: Rect,
    /// Volume block (owner request 2026-11-10): caption line, slider and mute button.
    pub volume_label: Rect,
    pub volume_slider: Rect,
    pub volume_mute: Rect,
    pub divider2: i32,
    pub rotation: Rect,
    pub theme: Rect,
    pub screen_off: Rect,
}

pub fn qs_geom(frame: &Frame) -> QsGeom {
    let x = frame.w - QS_W;
    let top = HEADER_H;
    let (ix, iw) = (x + QS_PAD, QS_W - 2 * QS_PAD);
    let mut y = top + QS_PAD;
    let mut row = || {
        let r = Rect::new(ix, y, iw, QS_ROW);
        y += QS_ROW;
        r
    };
    let (wifi, hotspot, mobile, bluetooth, gps, battery) =
        (row(), row(), row(), row(), row(), row());
    let divider1 = battery.bottom() + 4;
    let brightness_label = Rect::new(ix + SP_3, divider1 + 9, iw - 2 * SP_3, 24);
    let slider = Rect::new(ix, brightness_label.bottom(), iw, 56);
    let volume_label = Rect::new(ix + SP_3, slider.bottom() + 4, iw - 2 * SP_3, 24);
    let volume_slider = Rect::new(ix, volume_label.bottom(), iw - QS_ROW, 56);
    let volume_mute = Rect::new(volume_slider.right(), volume_label.bottom(), QS_ROW, 56);
    let divider2 = volume_slider.bottom() + 4;
    let mut y = divider2 + 5;
    let mut row = || {
        let r = Rect::new(ix, y, iw, QS_ROW);
        y += QS_ROW;
        r
    };
    // no Keyboard row: the keyboard opens where it is needed (terminal start, dock button)
    let (rotation, theme, screen_off) = (row(), row(), row());
    QsGeom {
        panel: Rect::new(x, top, QS_W, screen_off.bottom() + QS_PAD - top),
        wifi,
        hotspot,
        mobile,
        bluetooth,
        gps,
        battery,
        divider1,
        brightness_label,
        slider,
        volume_label,
        volume_slider,
        volume_mute,
        divider2,
        rotation,
        theme,
        screen_off,
    }
}

/// The header pill's Wi-Fi mark.
pub fn wifi_mark(m: &Model) -> WifiMark {
    match &m.status {
        None => WifiMark::Unknown,
        Some(s) if s.wifi.radio == WifiRadio::Failed => WifiMark::Failed,
        Some(s) if wifi_name(s).is_some() => WifiMark::Connected,
        Some(_) => WifiMark::Disconnected,
    }
}

/// Battery percent as shown in the pill: "62%" (voltage estimate and gauge look the same to the
/// owner, who asked for a plain number with no "~"; the estimate is still named in the Quick
/// Settings subtitle), "--" when unavailable.
pub fn battery_label(m: &Model) -> String {
    match m
        .status
        .as_ref()
        .map(|s| (s.battery.percent, s.battery.source))
    {
        Some((Some(p), _)) => format!("{p}%"),
        _ => "--".into(),
    }
}

/// "5 h 20 min left" / "1 h 10 min to full" (rounded to 5 min), "calculating..." while the current
/// data is too short, None when unknown (owner request 2026-11-11; no "~" - owner request
/// 2026-11-11b).
pub fn battery_time_text(b: &walpad_proto::Battery) -> Option<String> {
    let (min, tail) = match (b.time_left_min, b.time_to_full_min) {
        (Some(m), _) => (m, "left"),
        (None, Some(m)) => (m, "to full"),
        _ if b.time_calculating => return Some("calculating...".into()),
        _ => return None,
    };
    let m = (min + 2) / 5 * 5;
    let m = m.max(5);
    let (h, r) = (m / 60, m % 60);
    Some(match (h, r) {
        (0, r) => format!("{r} min {tail}"),
        (h, 0) => format!("{h} h {tail}"),
        (h, r) => format!("{h} h {r} min {tail}"),
    })
}

/// Quick Settings battery row: "Battery (62%)" + its caveat. Unavailable stays labelled, an
/// estimate stays tagged as one (in the subtitle only, not in the percent itself).
pub fn battery_row(m: &Model) -> (String, Option<(String, Color)>) {
    let Some(st) = &m.status else {
        return ("Battery (unavailable)".into(), None);
    };
    let b = &st.battery;
    let time = battery_time_text(b).map(|t| (t, text_secondary()));
    match (b.percent, b.source) {
        (Some(p), BatterySource::VoltageEstimate) => (
            format!("Battery ({p}%)"),
            time.or(Some(("Estimated from voltage".into(), warning()))),
        ),
        (Some(p), BatterySource::Gauge) => (
            format!("Battery ({p}%)"),
            time.or(b.state.clone().map(|s| (s, text_secondary()))),
        ),
        _ => ("Battery (unavailable)".into(), None),
    }
}

/// Quick Settings Wi-Fi row subtitle.
pub fn wifi_row(m: &Model) -> (String, Color) {
    match &m.status {
        None => ("Unknown".into(), text_secondary()),
        Some(s) if s.wifi.radio == WifiRadio::Failed => {
            ("Failed: restart to retry".into(), danger())
        }
        Some(s) if !s.wifi.enabled => ("Off".into(), text_secondary()),
        Some(s) if s.wifi.state.as_deref() == Some("hotspot") => {
            ("Paused (hotspot)".into(), warning())
        }
        Some(s) => match wifi_name(s) {
            Some(n) => (n, text_secondary()),
            None => ("Not connected".into(), text_secondary()),
        },
    }
}

/// Quick Settings Hotspot row subtitle.
pub fn hotspot_row(m: &Model) -> (String, Color) {
    let Some(h) = m.status.as_ref().map(|s| &s.hotspot) else {
        return ("Unknown".into(), text_secondary());
    };
    match h.state {
        HotspotState::On => {
            let n = h.clients.unwrap_or(0);
            (
                format!("On, {n} {}", if n == 1 { "device" } else { "devices" }),
                success(),
            )
        }
        HotspotState::Starting => ("Starting...".into(), warning()),
        HotspotState::Stopping => ("Stopping...".into(), text_secondary()),
        HotspotState::Failed => ("Failed".into(), danger()),
        HotspotState::Off if !h.has_secret => ("No password set".into(), text_secondary()),
        HotspotState::Off => ("Off".into(), text_secondary()),
    }
}
