//! The two popovers: Quick Settings (anchored under the header pill, right) and the Apps panel
//! (anchored bottom-left next to the sidebar). Geometry is pure; rendering returns hit areas. Both
//! close on a tap outside: their hit list starts with a full-screen `Dismiss` and the panel body
//! swallows taps between rows (`PanelBody`).
use crate::app::{Model, Target, Tile};
use crate::home::wifi_name;
use walpad_proto::{BatterySource, WifiRadio};
use walpad_ui::components::{Painter, WifiMark};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Color, Face, Icon, Rect, Style};

pub const QS_W: i32 = 280;
const QS_PAD: i32 = 8;
const QS_ROW: i32 = 56;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QsGeom {
    pub panel: Rect,
    pub wifi: Rect,
    pub bluetooth: Rect,
    pub battery: Rect,
    pub divider1: i32,
    pub brightness_label: Rect,
    pub slider: Rect,
    pub divider2: i32,
    pub rotation: Rect,
    pub keyboard: Rect,
    pub screen_off: Rect,
}

pub fn qs_geom(frame: &Frame) -> QsGeom {
    let x = frame.w - SP_4 - QS_W;
    let top = HEADER_H - 4;
    let (ix, iw) = (x + QS_PAD, QS_W - 2 * QS_PAD);
    let mut y = top + QS_PAD;
    let mut row = || {
        let r = Rect::new(ix, y, iw, QS_ROW);
        y += QS_ROW;
        r
    };
    let (wifi, bluetooth, battery) = (row(), row(), row());
    let divider1 = battery.bottom() + 4;
    let brightness_label = Rect::new(ix + SP_3, divider1 + 9, iw - 2 * SP_3, 24);
    let slider = Rect::new(ix, brightness_label.bottom(), iw, 56);
    let divider2 = slider.bottom() + 4;
    let mut y = divider2 + 5;
    let mut row = || {
        let r = Rect::new(ix, y, iw, QS_ROW);
        y += QS_ROW;
        r
    };
    let (rotation, keyboard, screen_off) = (row(), row(), row());
    QsGeom {
        panel: Rect::new(x, top, QS_W, screen_off.bottom() + QS_PAD - top),
        wifi,
        bluetooth,
        battery,
        divider1,
        brightness_label,
        slider,
        divider2,
        rotation,
        keyboard,
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

/// Battery percent as shown in the pill: "~62%" for a voltage estimate, "--" when unavailable.
pub fn battery_label(m: &Model) -> String {
    match m
        .status
        .as_ref()
        .map(|s| (s.battery.percent, s.battery.source))
    {
        Some((Some(p), BatterySource::VoltageEstimate)) => format!("~{p}%"),
        Some((Some(p), BatterySource::Gauge)) => format!("{p}%"),
        _ => "--".into(),
    }
}

/// Quick Settings battery row: "Battery (~62%)" + its caveat. Unavailable stays labelled, an
/// estimate stays tagged as one.
pub fn battery_row(m: &Model) -> (String, Option<(String, Color)>) {
    let Some(st) = &m.status else {
        return ("Battery (unavailable)".into(), None);
    };
    let b = &st.battery;
    match (b.percent, b.source) {
        (Some(p), BatterySource::VoltageEstimate) => (
            format!("Battery (~{p}%)"),
            Some(("Estimated from voltage".into(), WARNING)),
        ),
        (Some(p), BatterySource::Gauge) => (
            format!("Battery ({p}%)"),
            b.state.clone().map(|s| (s, TEXT_SECONDARY)),
        ),
        _ => ("Battery (unavailable)".into(), None),
    }
}

/// Quick Settings Wi-Fi row subtitle.
pub fn wifi_row(m: &Model) -> (String, Color) {
    match &m.status {
        None => ("Unknown".into(), TEXT_SECONDARY),
        Some(s) if s.wifi.radio == WifiRadio::Failed => ("Failed: restart to retry".into(), DANGER),
        Some(s) => match wifi_name(s) {
            Some(n) => (n, TEXT_SECONDARY),
            None => ("Not connected".into(), TEXT_SECONDARY),
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn qs_row(
    p: &mut Painter,
    r: Rect,
    icon: Icon,
    title: &str,
    sub: Option<(&str, Color)>,
    chevron: bool,
    pressed: bool,
    disabled: bool,
) {
    if pressed && !disabled {
        p.row_pressed(r);
    }
    let fg = if disabled { TEXT_DISABLED } else { TEXT };
    p.cv.icon(icon, r.x + SP_3, r.y + (r.h - ICON) / 2, ICON, fg);
    let tx = r.x + SP_3 + ICON + SP_3;
    let tw = r.right() - tx - if chevron { 32 } else { SP_3 };
    let style = Style::new(Face::Regular, SIZE_SECONDARY + 1.0, fg);
    match sub {
        Some((s, c)) => {
            p.text(title, style, Rect::new(tx, r.y + 7, tw, 22), Align::Left);
            p.text(
                s,
                Style::new(
                    Face::Regular,
                    SIZE_CAPTION - 1.0,
                    if disabled { TEXT_DISABLED } else { c },
                ),
                Rect::new(tx, r.y + 30, tw, 18),
                Align::Left,
            );
        }
        None => p.text(title, style, Rect::new(tx, r.y, tw, r.h), Align::Left),
    }
    if chevron {
        p.cv.icon(
            Icon::ChevronRight,
            r.right() - 28,
            r.y + (r.h - 20) / 2,
            20,
            TEXT_SECONDARY,
        );
    }
}

pub fn render_quick_settings(p: &mut Painter, frame: &Frame, m: &Model) -> Vec<(Rect, Target)> {
    let g = qs_geom(frame);
    let pr = |t: Target| m.pressed == Some(t);
    let online = m.connected;
    let mut hits = vec![
        (frame.screen(), Target::Dismiss),
        (g.panel, Target::PanelBody),
    ];
    p.popover(g.panel);

    let (wsub, wcol) = wifi_row(m);
    qs_row(
        p,
        g.wifi,
        Icon::Wifi,
        "Wi-Fi Network",
        Some((&wsub, wcol)),
        true,
        pr(Target::WifiCard),
        false,
    );
    hits.push((g.wifi, Target::WifiCard));
    qs_row(
        p,
        g.bluetooth,
        Icon::Bluetooth,
        "Bluetooth",
        Some(("Not supported on this tablet", TEXT_DISABLED)),
        false,
        false,
        true,
    );
    let (btitle, bsub) = battery_row(m);
    qs_row(
        p,
        g.battery,
        Icon::Battery,
        &btitle,
        bsub.as_ref().map(|(s, c)| (s.as_str(), *c)),
        true,
        pr(Target::BatteryCard),
        false,
    );
    hits.push((g.battery, Target::BatteryCard));
    p.divider(g.panel.x + SP_4, g.divider1, g.panel.w - 2 * SP_4);

    // no core status = no known brightness: show "--", not a made-up 0%
    let pct = if m.status.is_some() || m.slider_active {
        format!("{}%", (m.brightness_value() * 100.0).round() as i32)
    } else {
        "--".to_string()
    };
    let lc = if online { TEXT } else { TEXT_DISABLED };
    p.cv.icon(
        Icon::Sun,
        g.brightness_label.x,
        g.brightness_label.y,
        ICON,
        lc,
    );
    p.text(
        "Brightness",
        Style::new(Face::Regular, SIZE_SECONDARY + 1.0, lc),
        Rect::new(
            g.brightness_label.x + ICON + SP_3,
            g.brightness_label.y,
            g.brightness_label.w - ICON - SP_3,
            g.brightness_label.h,
        ),
        Align::Left,
    );
    p.text(
        &pct,
        Style::new(Face::Regular, SIZE_SECONDARY, TEXT_SECONDARY),
        g.brightness_label,
        Align::Right,
    );
    p.slider(g.slider, m.brightness_value(), m.slider_active, !online);
    hits.push((g.slider, Target::Slider));
    p.divider(g.panel.x + SP_4, g.divider2, g.panel.w - 2 * SP_4);

    let rot = if m.status.as_ref().is_some_and(|s| s.rotation_locked) {
        "Rotation: Locked"
    } else {
        "Rotation: Auto"
    };
    for (r, icon, label, t) in [
        (g.rotation, Icon::Rotate, rot, Target::Rotation),
        (g.keyboard, Icon::Keyboard, "Keyboard", Target::Keyboard),
        (g.screen_off, Icon::Screen, "Screen off", Target::ScreenOff),
    ] {
        qs_row(p, r, icon, label, None, false, pr(t), !online);
        hits.push((r, t));
    }
    hits
}

// ---------------------------------------------------------------------------------- Apps panel

pub const PANEL_W: i32 = 320;
const PANEL_PAD: i32 = 20;
const PANEL_COLS: i32 = 4;
const CELL_W: i32 = (PANEL_W - 2 * PANEL_PAD) / PANEL_COLS;
const CELL_H: i32 = BOX_PANEL + 6 + 18;
const CELL_GAP: i32 = SP_3;
const CAPTION_H: i32 = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelGeom {
    pub panel: Rect,
    pub active_label: Rect,
    pub active: Vec<Rect>,
    /// "No apps running" line when nothing runs.
    pub active_empty: Option<Rect>,
    pub divider: i32,
    pub installed_label: Rect,
    pub installed: Vec<Rect>,
}

fn grid_h(n: usize) -> i32 {
    let rows = (n as i32 + PANEL_COLS - 1) / PANEL_COLS;
    rows * CELL_H + (rows - 1).max(0) * CELL_GAP
}

/// Bottom-left next to the sidebar: the panel grows upwards from 16 px above the bottom edge.
pub fn panel_geom(frame: &Frame, running: usize, installed: usize) -> PanelGeom {
    let active_h = if running == 0 { 24 } else { grid_h(running) };
    let h = PANEL_PAD
        + CAPTION_H
        + SP_3
        + active_h
        + SP_4
        + 1
        + SP_4
        + CAPTION_H
        + SP_3
        + grid_h(installed)
        + PANEL_PAD;
    let x = SIDEBAR_W + SP_3;
    let top = (frame.h - SP_4 - h).max(HEADER_H + SP_2);
    let ix = x + PANEL_PAD;
    let cells = |y0: i32, n: usize| -> Vec<Rect> {
        (0..n as i32)
            .map(|i| {
                Rect::new(
                    ix + (i % PANEL_COLS) * CELL_W,
                    y0 + (i / PANEL_COLS) * (CELL_H + CELL_GAP),
                    CELL_W,
                    CELL_H,
                )
            })
            .collect()
    };
    let mut y = top + PANEL_PAD;
    let active_label = Rect::new(ix, y, PANEL_W - 2 * PANEL_PAD, CAPTION_H);
    y += CAPTION_H + SP_3;
    let active = cells(y, running);
    let active_empty = (running == 0).then(|| Rect::new(ix, y, PANEL_W - 2 * PANEL_PAD, 24));
    y += active_h + SP_4;
    let divider = y;
    y += 1 + SP_4;
    let installed_label = Rect::new(ix, y, PANEL_W - 2 * PANEL_PAD, CAPTION_H);
    y += CAPTION_H + SP_3;
    PanelGeom {
        panel: Rect::new(x, top, PANEL_W, h),
        active_label,
        active,
        active_empty,
        divider,
        installed_label,
        installed: cells(y, installed),
    }
}

/// Indices into `apps` of the running ones.
pub fn running(apps: &[Tile]) -> Vec<usize> {
    apps.iter()
        .enumerate()
        .filter(|(_, t)| t.running)
        .map(|(i, _)| i)
        .collect()
}

pub fn render_apps_panel(p: &mut Painter, frame: &Frame, m: &Model) -> Vec<(Rect, Target)> {
    let run = running(&m.apps);
    let g = panel_geom(frame, run.len(), m.apps.len());
    let mut hits = vec![
        (frame.screen(), Target::Dismiss),
        (g.panel, Target::PanelBody),
    ];
    p.popover(g.panel);
    p.caption_upper("Active apps", g.active_label);
    if let Some(r) = g.active_empty {
        p.text(
            "No apps running",
            Style::new(Face::Regular, SIZE_CAPTION, TEXT_DISABLED),
            r,
            Align::Left,
        );
    }
    for (cell, &i) in g.active.iter().zip(&run) {
        let t = &m.apps[i];
        let target = Target::PanelActive(i);
        p.app_icon(
            *cell,
            BOX_PANEL,
            t.icon,
            &t.name,
            m.pressed == Some(target),
            false,
        );
        hits.push((*cell, target));
    }
    p.divider(g.panel.x + PANEL_PAD, g.divider, PANEL_W - 2 * PANEL_PAD);
    p.caption_upper("Installed apps", g.installed_label);
    for (i, (cell, t)) in g.installed.iter().zip(&m.apps).enumerate() {
        let target = Target::PanelApp(i);
        p.app_icon(
            *cell,
            BOX_PANEL,
            t.icon,
            &t.name,
            m.pressed == Some(target),
            t.running,
        );
        hits.push((*cell, target));
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlaps(a: Rect, b: Rect) -> bool {
        a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
    }

    #[test]
    fn quick_settings_hang_under_the_pill_and_fit_both_orientations() {
        for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
            let g = qs_geom(&f);
            assert_eq!(g.panel.right(), f.w - SP_4, "right-aligned with the pill");
            assert!(g.panel.y < HEADER_H && g.panel.bottom() < f.h - SP_4);
            let rows = [
                g.wifi,
                g.bluetooth,
                g.battery,
                g.slider,
                g.rotation,
                g.keyboard,
                g.screen_off,
            ];
            for (i, a) in rows.iter().enumerate() {
                assert!(a.h >= TOUCH_MIN, "{a:?}");
                assert!(g.panel.contains(a.x, a.y) && a.right() <= g.panel.right());
                for b in &rows[i + 1..] {
                    assert!(!overlaps(*a, *b), "{a:?} overlaps {b:?}");
                }
            }
        }
    }

    #[test]
    fn apps_panel_sits_next_to_the_sidebar_at_the_bottom() {
        for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
            let g = panel_geom(&f, 2, 7);
            assert_eq!(g.panel.x, SIDEBAR_W + SP_3);
            assert_eq!(g.panel.bottom(), f.h - SP_4);
            assert_eq!(g.panel.w, PANEL_W);
            assert_eq!(g.installed.len(), 7);
            assert_eq!(
                g.installed
                    .iter()
                    .filter(|r| r.y == g.installed[0].y)
                    .count(),
                4,
                "four columns"
            );
            for r in g.active.iter().chain(&g.installed) {
                assert!(r.w >= TOUCH_MIN && r.h >= TOUCH_MIN, "{r:?}");
                assert!(r.right() <= g.panel.right() - PANEL_PAD + 1);
                assert!(r.bottom() <= g.panel.bottom());
            }
            assert!(g.active[0].bottom() < g.divider && g.divider < g.installed_label.y);
        }
        let none = panel_geom(&Frame::new(800, 1280), 0, 3);
        assert!(none.active.is_empty() && none.active_empty.is_some());
    }
}
