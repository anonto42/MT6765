use super::qs_model::{battery_row, hotspot_row, qs_geom, wifi_row};
use crate::app::{Model, Target};
use walpad_proto::HotspotState;
use walpad_ui::components::Painter;
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Color, Face, Icon, Rect, Style};

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
    let fg = if disabled { text_disabled() } else { text() };
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
                    if disabled { text_disabled() } else { c },
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
            text_secondary(),
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
    let lc = if online { text() } else { text_disabled() };
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
        Style::new(Face::Regular, SIZE_SECONDARY, text_secondary()),
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
    let theme_lbl = if theme() == ThemeMode::Dark {
        "Theme: Dark"
    } else {
        "Theme: Light"
    };
    let screen_lbl = if m.screen_off_armed_until.is_some() {
        "Tap again: Screen off"
    } else {
        "Screen off"
    };
    for (r, icon, label, t) in [
        (g.rotation, Icon::Rotate, rot, Target::Rotation),
        (g.theme, Icon::Sun, theme_lbl, Target::ThemeToggle),
        (g.screen_off, Icon::Screen, screen_lbl, Target::ScreenOff),
    ] {
        qs_row(p, r, icon, label, None, false, pr(t), !online);
        hits.push((r, t));
    }
    hits
}
