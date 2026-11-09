//! Home screen: layout (pure) and rendering. Fits portrait without scrolling.
use crate::app::{Model, Target, Tile};
use walpad_proto::{BatterySource, WifiRadio};
use walpad_ui::components::{ButtonKind, ButtonState, CardValue, Painter};
use walpad_ui::layout::{columns, Frame};
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Icon, Rect, Style};

#[derive(Debug, Clone, PartialEq)]
pub struct HomeLayout {
    pub date: Rect,
    pub clock: Rect,
    pub wifi: Rect,
    pub battery: Rect,
    pub label: Rect,
    pub tiles: Vec<Rect>,
    pub controls: Rect,
    pub brightness_label: Rect,
    pub slider: Rect,
    pub buttons: [Rect; 3],
    pub bottom: i32,
}

const CARD_H: i32 = 112;
const TILE_H: i32 = 160;
const CLOCK_H: i32 = 110;
const CONTROLS_H: i32 = SP_4 + 24 + 56 + SP_4 + 56 + SP_4;

/// `banner_h` is the height reserved at the top of the content for a persistent banner.
pub fn layout(frame: &Frame, banner_h: i32, tile_count: usize) -> HomeLayout {
    let c = frame.padded();
    let mut y = c.y + banner_h + SP_2;
    // landscape: the clock takes the first two columns, the cards sit beside it
    let clock_w = if frame.landscape() {
        c.w - 2 * ((c.w - 3 * SP_5) / 4) - 2 * SP_5
    } else {
        c.w
    };
    let date = Rect::new(c.x, y, clock_w, 24);
    let clock = Rect::new(c.x, y + 24, clock_w, CLOCK_H);
    let (wifi, battery);
    if frame.landscape() {
        let cols = columns(Rect::new(c.x, y, c.w, CARD_H), 4, SP_5);
        wifi = Rect::new(cols[2].x, y + 24, cols[2].w, CARD_H);
        battery = Rect::new(cols[3].x, y + 24, cols[3].w, CARD_H);
        y += 24 + CLOCK_H + SP_4;
    } else {
        y += 24 + CLOCK_H + SP_4;
        let cols = columns(Rect::new(c.x, y, c.w, CARD_H), 2, SP_5);
        wifi = cols[0];
        battery = cols[1];
        y += CARD_H + SP_5;
    }
    let label = Rect::new(c.x, y, c.w, 24);
    y += 24 + SP_2;
    let n_cols = frame.columns();
    let rows = (tile_count as i32 + n_cols - 1) / n_cols;
    let mut tiles = Vec::new();
    for i in 0..tile_count as i32 {
        let col = columns(Rect::new(c.x, 0, c.w, TILE_H), n_cols, SP_5);
        let r = col[(i % n_cols) as usize];
        tiles.push(Rect::new(
            r.x,
            y + (i / n_cols) * (TILE_H + SP_5),
            r.w,
            TILE_H,
        ));
    }
    y += rows.max(1) * (TILE_H + SP_5);
    let controls = Rect::new(c.x, y, c.w, CONTROLS_H);
    let inner = controls.inset(SP_4);
    let brightness_label = Rect::new(inner.x, inner.y, inner.w, 24);
    let slider = Rect::new(controls.x, inner.y + 24, controls.w, 56);
    let b = columns(
        Rect::new(inner.x, slider.bottom() + SP_4, inner.w, 56),
        3,
        SP_3,
    );
    HomeLayout {
        date,
        clock,
        wifi,
        battery,
        label,
        tiles,
        controls,
        brightness_label,
        slider,
        buttons: [b[0], b[1], b[2]],
        bottom: controls.bottom(),
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

/// Draws Home and returns the hit rectangles.
pub fn render(p: &mut Painter, frame: &Frame, m: &Model, banner_h: i32) -> Vec<(Rect, Target)> {
    let l = layout(frame, banner_h, m.tiles.len());
    let pressed = |t: Target| m.pressed == Some(t);
    let mut hits = Vec::new();

    p.text(
        &m.wall.date.to_uppercase(),
        Style::new(Face::Regular, SIZE_SECONDARY, TEXT_SECONDARY),
        l.date,
        Align::Left,
    );
    // 12-hour mode shows "2:22" (no leading zero) with AM/PM beside it
    let clock = if m.wall.suffix.is_some() {
        format!("{}:{:02}", m.wall.hour, m.wall.minute)
    } else {
        format!("{:02}:{:02}", m.wall.hour, m.wall.minute)
    };
    p.text(
        &clock,
        Style::new(Face::ExtraLight, SIZE_CLOCK, TEXT),
        l.clock,
        Align::Left,
    );
    if let Some(sfx) = m.wall.suffix {
        let w = p.fonts.measure(&clock, Face::ExtraLight, SIZE_CLOCK) as i32;
        p.text(
            sfx,
            Style::new(Face::Regular, SIZE_BODY, TEXT_SECONDARY),
            Rect::new(l.clock.x + w + SP_3, l.clock.y + 40, 60, 28),
            Align::Left,
        );
    }

    let (wv, wd) = wifi_card(m);
    p.status_card(
        l.wifi,
        "Wi-Fi",
        &wv,
        wd.as_deref(),
        pressed(Target::WifiCard),
    );
    hits.push((l.wifi, Target::WifiCard));
    let (bt, bv) = battery_card(m);
    p.status_card(l.battery, &bt, &bv, None, pressed(Target::BatteryCard));
    hits.push((l.battery, Target::BatteryCard));

    p.section_label("Apps", l.label);
    for (i, (r, t)) in l.tiles.iter().zip(&m.tiles).enumerate() {
        let tile: &Tile = t;
        p.app_tile(
            *r,
            tile.icon,
            &tile.name,
            pressed(Target::Tile(i)),
            tile.running,
        );
        hits.push((*r, Target::Tile(i)));
    }

    p.card(l.controls, false);
    // no core status = no known brightness: show "--", not a made-up 0%
    let pct_text = if m.status.is_some() || m.slider_active {
        format!("{}%", (m.brightness_value() * 100.0).round() as i32)
    } else {
        "--".to_string()
    };
    p.text(
        "Brightness",
        Style::new(Face::Bold, SIZE_SECONDARY, TEXT_SECONDARY),
        l.brightness_label,
        Align::Left,
    );
    p.text(
        &pct_text,
        Style::new(Face::Regular, SIZE_SECONDARY, TEXT_SECONDARY),
        l.brightness_label,
        Align::Right,
    );
    p.slider(
        l.slider,
        m.brightness_value(),
        m.slider_active,
        !m.connected,
    );
    hits.push((l.slider, Target::Slider));

    let state = |t: Target| {
        if !m.connected {
            ButtonState::Disabled
        } else if pressed(t) {
            ButtonState::Pressed
        } else {
            ButtonState::Default
        }
    };
    let rot = if m.status.as_ref().is_some_and(|s| s.rotation_locked) {
        "Rotation: Locked"
    } else {
        "Rotation: Auto"
    };
    p.button(
        l.buttons[0],
        rot,
        ButtonKind::Secondary,
        state(Target::Rotation),
        Some(Icon::Rotate),
    );
    p.button(
        l.buttons[1],
        "Keyboard",
        ButtonKind::Secondary,
        state(Target::Keyboard),
        Some(Icon::Keyboard),
    );
    p.button(
        l.buttons[2],
        "Screen off",
        ButtonKind::Secondary,
        state(Target::ScreenOff),
        Some(Icon::Screen),
    );
    hits.push((l.buttons[0], Target::Rotation));
    hits.push((l.buttons[1], Target::Keyboard));
    hits.push((l.buttons[2], Target::ScreenOff));
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlaps(a: Rect, b: Rect) -> bool {
        a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
    }

    fn all_rects(l: &HomeLayout) -> Vec<Rect> {
        let mut v = vec![l.date, l.clock, l.wifi, l.battery, l.label, l.controls];
        v.extend(l.tiles.iter().copied());
        v
    }

    #[test]
    fn portrait_fits_without_scrolling_even_with_a_banner() {
        let f = Frame::new(800, 1280);
        for banner in [0, 56] {
            let l = layout(&f, banner, 4);
            assert!(
                l.bottom <= f.content.bottom(),
                "bottom {} must be <= 1192 (banner {banner})",
                l.bottom
            );
            assert_eq!(l.tiles.len(), 4);
            assert_eq!(
                (l.tiles[0].w, l.tiles[0].h),
                (364, 160),
                "2x2 tiles 364x160 as in the spec"
            );
            assert_eq!((l.wifi.w, l.wifi.h), (364, 112));
            let rs = all_rects(&l);
            for (i, a) in rs.iter().enumerate() {
                assert!(
                    a.x >= 0 && a.right() <= 800 && a.y >= f.content.y,
                    "{a:?} inside the content area"
                );
                for b in &rs[i + 1..] {
                    assert!(!overlaps(*a, *b), "{a:?} overlaps {b:?}");
                }
            }
            assert!(
                l.slider.h >= TOUCH_MIN
                    && l.buttons
                        .iter()
                        .all(|b| b.h >= TOUCH_MIN && b.w >= TOUCH_MIN)
            );
        }
    }

    #[test]
    fn landscape_uses_four_columns_and_fits() {
        let f = Frame::new(1280, 800);
        let l = layout(&f, 56, 4);
        assert!(l.bottom <= f.content.bottom(), "bottom {}", l.bottom);
        assert_eq!(
            l.tiles
                .iter()
                .map(|t| t.y)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            1,
            "one row of four"
        );
        let rs = all_rects(&l);
        for (i, a) in rs.iter().enumerate() {
            for b in &rs[i + 1..] {
                assert!(!overlaps(*a, *b), "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn wifi_card_is_honest_about_what_is_known() {
        use crate::app::{AppEntry, WallTime};
        let _ = AppEntry {
            id: String::new(),
            name: String::new(),
            icon: Icon::Apps,
        };
        let mut st = crate::app::tests::status(120, &[]);
        let model = |st: walpad_proto::Status| {
            let mut m = Model::blank(WallTime::new(1, 1, "x", true));
            m.status = Some(st);
            m.connected = true;
            m
        };
        assert_eq!(
            wifi_card(&model(st.clone())).0,
            CardValue::Value("Home".into())
        );
        st.wifi.ssid = None;
        assert_eq!(
            wifi_card(&model(st.clone())),
            (
                CardValue::Value("Connected".into()),
                Some("192.168.0.100".into())
            ),
            "address but no SSID"
        );
        st.wifi.ip = None;
        assert_eq!(
            wifi_card(&model(st.clone())),
            (CardValue::Value("Not connected".into()), None)
        );
        st.wifi.radio = WifiRadio::Failed;
        assert_eq!(
            wifi_card(&model(st)).0,
            CardValue::Error("Wi-Fi failed".into())
        );
    }

    #[test]
    fn fewer_tiles_still_lay_out() {
        for n in 0..=4 {
            let l = layout(&Frame::new(800, 1280), 0, n);
            assert_eq!(l.tiles.len(), n);
            assert!(l.bottom <= 1192);
        }
    }
}
