//! The two popovers: Quick Settings (anchored under the header pill, right) and the Apps panel
//! (anchored bottom-left next to the sidebar). Geometry is pure; rendering returns hit areas. Both
//! close on a tap outside: their hit list starts with a full-screen `Dismiss` and the panel body
//! swallows taps between rows (`PanelBody`).
use crate::app::{Model, Target, Tile};
use crate::home::wifi_name;
use walpad_proto::{BatterySource, HotspotState, WifiRadio};
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
    let (rotation, screen_off) = (row(), row());
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
    let time = battery_time_text(b).map(|t| (t, TEXT_SECONDARY));
    match (b.percent, b.source) {
        (Some(p), BatterySource::VoltageEstimate) => (
            format!("Battery ({p}%)"),
            time.or(Some(("Estimated from voltage".into(), WARNING))),
        ),
        (Some(p), BatterySource::Gauge) => (
            format!("Battery ({p}%)"),
            time.or(b.state.clone().map(|s| (s, TEXT_SECONDARY))),
        ),
        _ => ("Battery (unavailable)".into(), None),
    }
}

/// Quick Settings Wi-Fi row subtitle.
pub fn wifi_row(m: &Model) -> (String, Color) {
    match &m.status {
        None => ("Unknown".into(), TEXT_SECONDARY),
        Some(s) if s.wifi.radio == WifiRadio::Failed => ("Failed: restart to retry".into(), DANGER),
        Some(s) if !s.wifi.enabled => ("Off".into(), TEXT_SECONDARY),
        Some(s) if s.wifi.state.as_deref() == Some("hotspot") => {
            ("Paused (hotspot)".into(), WARNING)
        }
        Some(s) => match wifi_name(s) {
            Some(n) => (n, TEXT_SECONDARY),
            None => ("Not connected".into(), TEXT_SECONDARY),
        },
    }
}

/// Quick Settings Hotspot row subtitle.
pub fn hotspot_row(m: &Model) -> (String, Color) {
    let Some(h) = m.status.as_ref().map(|s| &s.hotspot) else {
        return ("Unknown".into(), TEXT_SECONDARY);
    };
    match h.state {
        HotspotState::On => {
            let n = h.clients.unwrap_or(0);
            (
                format!("On, {n} {}", if n == 1 { "device" } else { "devices" }),
                SUCCESS,
            )
        }
        HotspotState::Starting => ("Starting...".into(), WARNING),
        HotspotState::Stopping => ("Stopping...".into(), TEXT_SECONDARY),
        HotspotState::Failed => ("Failed".into(), DANGER),
        HotspotState::Off if !h.has_secret => ("No password set".into(), TEXT_SECONDARY),
        HotspotState::Off => ("Off".into(), TEXT_SECONDARY),
    }
}

/// A Quick Settings row with a switch at the right: the row opens `row_t`, the switch toggles
/// (`switch_t`, drawn last so its hit area wins).
#[allow(clippy::too_many_arguments)]
fn qs_switch_row(
    p: &mut Painter,
    hits: &mut Vec<(Rect, Target)>,
    m: &Model,
    r: Rect,
    icon: Icon,
    title: &str,
    sub: (&str, Color),
    on: bool,
    row_t: Target,
    switch_t: Target,
    switch_disabled: bool,
) {
    if m.pressed == Some(row_t) {
        p.row_pressed(r);
    }
    let sw_x = r.right() - SP_3 - 56;
    let text_r = Rect::new(r.x, r.y, sw_x - SP_2 - r.x, r.h);
    qs_row(p, text_r, icon, title, Some(sub), false, false, false);
    p.toggle(
        sw_x,
        r.y + (r.h - 32) / 2,
        on,
        m.pressed == Some(switch_t),
        switch_disabled,
    );
    hits.push((r, row_t));
    hits.push((
        Rect::new(sw_x - SP_3, r.y, r.right() - sw_x + SP_3, r.h),
        switch_t,
    ));
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

    // Wi-Fi and Hotspot: tap the row = Network screen, the switch toggles (owner request 2026-11-10)
    let sw_off = !online || m.status.is_none();
    let (wsub, wcol) = wifi_row(m);
    qs_switch_row(
        p,
        &mut hits,
        m,
        g.wifi,
        Icon::Wifi,
        "Wi-Fi Network",
        (&wsub, wcol),
        crate::network::wifi_enabled(&m.status),
        Target::WifiCard,
        Target::WifiSwitch,
        sw_off,
    );
    let (hsub, hcol) = hotspot_row(m);
    let hs_busy = m.status.as_ref().is_some_and(|s| {
        matches!(
            s.hotspot.state,
            HotspotState::Starting | HotspotState::Stopping
        )
    });
    qs_switch_row(
        p,
        &mut hits,
        m,
        g.hotspot,
        Icon::Globe,
        "Hotspot",
        (&hsub, hcol),
        crate::network::hotspot_on(&m.status),
        Target::HotspotCard,
        Target::HotspotSwitch,
        sw_off || hs_busy,
    );
    // Mobile data: the row opens the Mobile network screen, signal bars beside the switch
    let (msub, mcol) = crate::mobile::qs_line(&m.status);
    qs_switch_row(
        p,
        &mut hits,
        m,
        g.mobile,
        Icon::Sim,
        "Mobile data",
        (&msub, mcol),
        crate::mobile::data_on(&m.status),
        Target::MobileCard,
        Target::MobileDataSwitch,
        sw_off || !crate::mobile::data_switchable(&m.status),
    );
    if let Some((bars, c)) = crate::mobile::pill_cell(&m.status) {
        let sw_x = g.mobile.right() - SP_3 - 56;
        p.cell_bars(
            sw_x - SP_3 - walpad_ui::components::CELL_BARS_W,
            g.mobile.y + 9,
            bars,
            c,
        );
    }
    let (bsub_t, bcol) = crate::bluetooth::qs_line(&m.status);
    qs_switch_row(
        p,
        &mut hits,
        m,
        g.bluetooth,
        Icon::Bluetooth,
        "Bluetooth",
        (&bsub_t, bcol),
        crate::bluetooth::switch_on(&m.status),
        Target::BtCard,
        Target::BtSwitch,
        sw_off || !crate::bluetooth::switchable(&m.status),
    );
    let (gsub, gcol) = crate::gps_screen::qs_line(&m.status);
    qs_switch_row(
        p,
        &mut hits,
        m,
        g.gps,
        Icon::Location,
        "GPS",
        (&gsub, gcol),
        crate::gps_screen::switch_on(&m.status),
        Target::GpsCard,
        Target::GpsSwitch,
        sw_off || !crate::gps_screen::switchable(&m.status),
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
    p.divider(g.panel.x, g.divider1, g.panel.w);

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
    crate::volume_ui::render_qs(
        p,
        m,
        g.volume_label,
        g.volume_slider,
        g.volume_mute,
        &mut hits,
    );
    p.divider(g.panel.x, g.divider2, g.panel.w);

    let rot = if m.status.as_ref().is_some_and(|s| s.rotation_locked) {
        "Rotation: Locked"
    } else {
        "Rotation: Auto"
    };
    for (r, icon, label, t) in [
        (g.rotation, Icon::Rotate, rot, Target::Rotation),
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
/// Close badge on a running app: drawn 20 px round, tapped through a 40x40 rectangle.
pub const BADGE_R: i32 = 10;
pub const BADGE_HIT: i32 = 40;

/// What a tool in the Apps panel's TOOLS grid does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAction {
    /// Open the mouse pad (a touchpad over the apps: mousepad.rs).
    MousePad,
    /// Show the clipboard history (seltools.rs).
    Clipboard,
    /// Open the app store at the catalogue (Settings > Apps).
    AppStore,
    /// Open the Media screen (pictures, videos, music).
    Media,
    /// Open the Phone screen (owner request 2026-11-11); the tile carries the unread badge.
    Phone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tool {
    pub icon: Icon,
    pub label: &'static str,
    pub action: ToolAction,
}

/// The TOOLS section, in grid order. Add a tool here (and its action in `App::on_tap`).
pub const TOOLS: [Tool; 5] = [
    Tool {
        icon: Icon::Mouse,
        label: "Mouse pad",
        action: ToolAction::MousePad,
    },
    Tool {
        icon: Icon::Clipboard,
        label: "Clipboard",
        action: ToolAction::Clipboard,
    },
    Tool {
        icon: Icon::Apps,
        label: "App store",
        action: ToolAction::AppStore,
    },
    Tool {
        icon: Icon::Media,
        label: "Media",
        action: ToolAction::Media,
    },
    Tool {
        icon: Icon::Phone,
        label: "Phone",
        action: ToolAction::Phone,
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelGeom {
    pub panel: Rect,
    pub active_label: Rect,
    pub active: Vec<Rect>,
    /// "No apps running" line when nothing runs.
    pub active_empty: Option<Rect>,
    pub divider: i32,
    pub tools_label: Rect,
    pub tools: Vec<Rect>,
}

fn grid_h(n: usize) -> i32 {
    let rows = (n as i32 + PANEL_COLS - 1) / PANEL_COLS;
    rows * CELL_H + (rows - 1).max(0) * CELL_GAP
}

/// The icon box of a panel cell (48 px, centred at the top of the cell).
pub fn cell_box(cell: Rect) -> Rect {
    Rect::new(
        cell.x + (cell.w - BOX_PANEL) / 2,
        cell.y,
        BOX_PANEL,
        BOX_PANEL,
    )
}

/// The close badge of a running app's cell: (drawn circle bounds, enlarged hit rectangle). The
/// badge sits on the icon box's top-right corner; the hit rectangle stays clear of the box centre,
/// so a tap on the icon itself still focuses the app.
pub fn close_badge(cell: Rect) -> (Rect, Rect) {
    let b = cell_box(cell);
    let (cx, cy) = (b.right() - 2, b.y + 2);
    (
        Rect::new(cx - BADGE_R, cy - BADGE_R, 2 * BADGE_R, 2 * BADGE_R),
        Rect::new(cx - BADGE_HIT / 2, cy - BADGE_HIT / 2, BADGE_HIT, BADGE_HIT),
    )
}

/// Bottom-left flush against the sidebar and bottom edge.
pub fn panel_geom(frame: &Frame, running: usize, tools: usize) -> PanelGeom {
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
        + grid_h(tools)
        + PANEL_PAD;
    let x = SIDEBAR_W;
    let top = (frame.h - h).max(HEADER_H);
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
    let tools_label = Rect::new(ix, y, PANEL_W - 2 * PANEL_PAD, CAPTION_H);
    y += CAPTION_H + SP_3;
    PanelGeom {
        panel: Rect::new(x, top, PANEL_W, h),
        active_label,
        active,
        active_empty,
        divider,
        tools_label,
        tools: cells(y, tools),
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

/// A panel cell: the 48 px icon box (translucent `SURFACE_ICON` over the panel fill) and its name.
fn panel_icon(p: &mut Painter, cell: Rect, icon: Icon, name: &str, pressed: bool) {
    p.app_icon(cell, BOX_PANEL, icon, name, pressed, false);
}

fn panel_caption(p: &mut Painter, s: &str, r: Rect) {
    p.caption_upper(s, r);
}

/// Square close badge: overlaps the icon box's corner with a dark fill, no outline.
fn paint_close_badge(p: &mut Painter, r: Rect, pressed: bool) {
    p.cv.rect(r, Tint::new(Color::hex(0x000000), 80));
    if pressed {
        p.cv.rect(r, PRESSED_TINT);
    }
    p.cv.icon(Icon::Close, r.x + 4, r.y + 4, r.w - 8, TEXT);
}

/// The Apps panel's outline rectangle for the current model.
pub fn apps_panel_rect(frame: &Frame, m: &Model) -> Rect {
    panel_geom(frame, running(&m.apps).len(), TOOLS.len()).panel
}

pub fn render_apps_panel(p: &mut Painter, frame: &Frame, m: &Model) -> Vec<(Rect, Target)> {
    let run = running(&m.apps);
    let g = panel_geom(frame, run.len(), TOOLS.len());
    let mut hits = vec![
        (frame.screen(), Target::Dismiss),
        (g.panel, Target::PanelBody),
    ];
    // translucent (owner request 2026-11-10): SURFACE_STRONG over the wallpaper, no scrim
    p.popover(g.panel);
    panel_caption(p, "Active apps", g.active_label);
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
        panel_icon(p, *cell, t.icon, &t.name, m.pressed == Some(target));
        hits.push((*cell, target));
    }
    // the badges after every icon: their hit rectangles overlap the neighbouring cell and must win
    // the hit test (it searches from the end)
    for (cell, &i) in g.active.iter().zip(&run) {
        let (badge, hit) = close_badge(*cell);
        paint_close_badge(p, badge, m.pressed == Some(Target::PanelClose(i)));
        hits.push((hit, Target::PanelClose(i)));
    }
    p.divider(g.panel.x, g.divider, PANEL_W);
    panel_caption(p, "Tools", g.tools_label);
    for (i, (cell, t)) in g.tools.iter().zip(TOOLS.iter()).enumerate() {
        let target = Target::PanelTool(i);
        panel_icon(p, *cell, t.icon, t.label, m.pressed == Some(target));
        if t.action == ToolAction::Phone {
            crate::phone_ui::paint_badge(p, cell_box(*cell), crate::phone_ui::unread(&m.status));
        }
        hits.push((*cell, target));
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_time_text_rounds_to_five_minutes() {
        let mut b = walpad_proto::Battery {
            percent: Some(50),
            source: BatterySource::VoltageEstimate,
            state: None,
            mv: None,
            temp_c: None,
            low: false,
            time_left_min: Some(322),
            time_to_full_min: None,
            time_calculating: false,
        };
        assert_eq!(battery_time_text(&b).unwrap(), "5 h 20 min left");
        b.time_left_min = Some(120);
        assert_eq!(battery_time_text(&b).unwrap(), "2 h left");
        b.time_left_min = Some(40);
        assert_eq!(battery_time_text(&b).unwrap(), "40 min left");
        b.time_left_min = None;
        b.time_to_full_min = Some(70);
        assert_eq!(battery_time_text(&b).unwrap(), "1 h 10 min to full");
        b.time_to_full_min = None;
        assert_eq!(battery_time_text(&b), None);
        b.time_calculating = true;
        assert_eq!(battery_time_text(&b).unwrap(), "calculating...");
    }

    fn overlaps(a: Rect, b: Rect) -> bool {
        a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
    }

    #[test]
    fn quick_settings_hang_under_the_pill_and_fit_both_orientations() {
        for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
            let g = qs_geom(&f);
            assert_eq!(g.panel.right(), f.w, "flush with the right edge");
            assert!(g.panel.y == HEADER_H && g.panel.bottom() < f.h - SP_4);
            let rows = [
                g.wifi,
                g.hotspot,
                g.mobile,
                g.bluetooth,
                g.battery,
                g.slider,
                g.volume_slider,
                g.volume_mute,
                g.rotation,
                g.screen_off,
            ];
            for (i, a) in rows.iter().enumerate() {
                assert!(a.h >= TOUCH_MIN, "{a:?}");
                assert!(g.panel.contains(a.x, a.y) && a.right() <= g.panel.right());
                for b in &rows[i + 1..] {
                    assert!(!overlaps(*a, *b), "{a:?} overlaps {b:?}");
                }
            }
            // Rotation and Screen off are the last two rows (no Keyboard row between them)
            assert_eq!(g.screen_off.y, g.rotation.bottom());
            assert_eq!(g.panel.bottom(), g.screen_off.bottom() + QS_PAD);
        }
    }

    #[test]
    fn apps_panel_sits_next_to_the_sidebar_at_the_bottom() {
        for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
            let g = panel_geom(&f, 2, 7);
            assert_eq!(g.panel.x, SIDEBAR_W);
            assert_eq!(g.panel.bottom(), f.h);
            assert_eq!(g.panel.w, PANEL_W);
            assert_eq!(g.tools.len(), 7);
            assert_eq!(
                g.tools.iter().filter(|r| r.y == g.tools[0].y).count(),
                4,
                "four columns"
            );
            for r in g.active.iter().chain(&g.tools) {
                assert!(r.w >= TOUCH_MIN && r.h >= TOUCH_MIN, "{r:?}");
                assert!(r.right() <= g.panel.right() - PANEL_PAD + 1);
                assert!(r.bottom() <= g.panel.bottom());
            }
            assert!(g.active[0].bottom() < g.divider && g.divider < g.tools_label.y);
        }
        let none = panel_geom(&Frame::new(800, 1280), 0, 3);
        assert!(none.active.is_empty() && none.active_empty.is_some());
    }

    #[test]
    fn close_badges_are_big_enough_and_leave_the_icon_centre_alone() {
        let g = panel_geom(&Frame::new(800, 1280), 4, TOOLS.len());
        for cell in &g.active {
            let (badge, hit) = close_badge(*cell);
            let b = cell_box(*cell);
            assert!(hit.w >= 40 && hit.h >= 40, "touch target >= 40 px");
            let (bx, by) = badge.center();
            // the badge sits on the box's top-right corner
            assert!(b.contains(bx - 1, by + 1) && !b.contains(bx + 2, by - 3));
            assert!(hit.contains(bx, by));
            // the icon centre (and the box's left half) still belong to the icon
            let (cx, cy) = b.center();
            assert!(!hit.contains(cx, cy) && !hit.contains(b.x + 2, b.y + 2));
            // inside the panel
            assert!(g.panel.contains(hit.x, hit.y) && hit.right() <= g.panel.right());
        }
    }

    #[test]
    fn tools_grid_is_generic() {
        assert_eq!(TOOLS[0].label, "Mouse pad");
        assert_eq!(TOOLS[0].action, ToolAction::MousePad);
        assert_eq!(TOOLS[0].icon, Icon::Mouse);
        let g = panel_geom(&Frame::new(800, 1280), 1, TOOLS.len());
        assert_eq!(g.tools.len(), TOOLS.len());
    }
}
