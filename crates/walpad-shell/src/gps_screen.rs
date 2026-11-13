//! GPS screen (owner request 2026-11-11): the switch, the fix state, the coordinates with Copy
//! buttons, satellite signal bars and the age of the last fix. Model helpers are pure; content
//! coordinates as in mobile.rs (y starts at 0 at the top of the scrollable content). The position is
//! private: it is drawn for the owner and copied only when a Copy button is tapped, never logged.
use crate::app::Target;
use crate::network::switch_hit;
use walpad_proto::{GpsState, GpsStatus, Status};
use walpad_ui::components::{Accessory, ButtonKind, ButtonState, Painter};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Color, Face, Icon, Rect, Style};

const TITLE_H: i32 = 40;
const STATE_CARD_H: i32 = 104;
const LABEL_H: i32 = 24;
const POS_CARD_H: i32 = 176;
const SAT_CARD_H: i32 = 208;
const NOTE_H: i32 = 72;
/// A position older than this is shown as "last known", not as current.
pub const FRESH_S: u32 = 10;

fn gps(st: &Option<Status>) -> Option<&GpsStatus> {
    st.as_ref().map(|s| &s.gps)
}

/// The owner's switch as the core reports it (unknown = off).
pub fn switch_on(st: &Option<Status>) -> bool {
    gps(st).is_some_and(|g| g.enabled)
}

pub fn busy(st: &Option<Status>) -> bool {
    gps(st).is_some_and(|g| matches!(g.state, GpsState::Starting | GpsState::Stopping))
}

/// The switch can be flipped: the core answers and no start/stop is running.
pub fn switchable(st: &Option<Status>) -> bool {
    st.is_some() && !busy(st)
}

/// Header pill mark while the GPS is on: white with a fix, dim while it looks for satellites.
pub fn pill_mark(st: &Option<Status>) -> Option<Color> {
    let g = gps(st)?;
    match g.state {
        GpsState::Fix => Some(text()),
        GpsState::Searching | GpsState::Starting => Some(text_disabled()),
        _ => None,
    }
}

/// State chip on the screen title.
pub fn chip(st: &Option<Status>) -> (&'static str, Color) {
    match gps(st).map(|g| g.state) {
        None => ("Unknown", text_disabled()),
        Some(GpsState::Off) => ("Off", text_disabled()),
        Some(GpsState::Starting) => ("Starting", warning()),
        Some(GpsState::Stopping) => ("Stopping", text_secondary()),
        Some(GpsState::Searching) => ("Searching", warning()),
        Some(GpsState::Fix) => ("Fix", success()),
        Some(GpsState::Failed) => ("Failed", danger()),
    }
}

/// Quick Settings row subtitle.
pub fn qs_line(st: &Option<Status>) -> (String, Color) {
    let Some(g) = gps(st) else {
        return ("Unknown".into(), text_secondary());
    };
    match g.state {
        GpsState::Off => ("Off".into(), text_secondary()),
        GpsState::Starting => ("Starting...".into(), warning()),
        GpsState::Stopping => ("Stopping...".into(), text_secondary()),
        GpsState::Searching => (
            match g.satellites_view {
                Some(n) if n > 0 => format!("Searching, {n} satellites"),
                _ => "Searching...".into(),
            },
            warning(),
        ),
        GpsState::Fix => (
            match g.accuracy_m {
                Some(a) => format!("Fix, within {a} m"),
                None => "Fix".into(),
            },
            success(),
        ),
        GpsState::Failed => ("Failed".into(), danger()),
    }
}

/// "23.75123, 90.40988" (5 decimals: about a metre).
pub fn coords_text(g: &GpsStatus) -> Option<String> {
    Some(format!("{:.5}, {:.5}", g.lat?, g.lon?))
}

/// OpenStreetMap link for the position (opened by pasting it into the browser).
pub fn map_link(g: &GpsStatus) -> Option<String> {
    let (la, lo) = (g.lat?, g.lon?);
    Some(format!(
        "https://www.openstreetmap.org/?mlat={la:.5}&mlon={lo:.5}#map=17/{la:.5}/{lo:.5}"
    ))
}

/// "just now", "12 s ago", "3 min ago", "over an hour ago".
pub fn age_text(age_s: u32) -> String {
    match age_s {
        0..=2 => "just now".into(),
        3..=59 => format!("{age_s} s ago"),
        60..=3599 => format!("{} min ago", age_s / 60),
        _ => "over an hour ago".into(),
    }
}

/// Card title, subtitle and its colour.
pub fn state_lines(st: &Option<Status>) -> (String, String, Color) {
    let Some(g) = gps(st) else {
        return (
            "Unknown".into(),
            "No status from the system".into(),
            text_disabled(),
        );
    };
    let sats = match (g.satellites_used, g.satellites_view) {
        (Some(u), Some(v)) => format!("{u} of {v} satellites used"),
        (None, Some(v)) => format!("{v} satellites in view"),
        _ => "No satellites yet".into(),
    };
    match g.state {
        GpsState::Off => (
            "GPS is off".into(),
            "Turn it on to find your position".into(),
            text_secondary(),
        ),
        GpsState::Starting => (
            "Starting the GPS".into(),
            "This takes a few seconds".into(),
            warning(),
        ),
        GpsState::Stopping => ("Stopping the GPS".into(), String::new(), text_secondary()),
        GpsState::Searching => (
            "Searching for satellites".into(),
            format!("{sats}. A first fix can take minutes"),
            warning(),
        ),
        GpsState::Fix => (
            "Position found".into(),
            match g.accuracy_m {
                Some(a) => format!("Within {a} m, {sats}"),
                None => sats,
            },
            success(),
        ),
        GpsState::Failed => (
            "GPS failed".into(),
            g.reason.clone().unwrap_or_else(|| "unknown error".into()),
            danger(),
        ),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GpsLayout {
    pub back: Rect,
    pub title: Rect,
    pub chip: Rect,
    pub state_card: Rect,
    pub switch_row: Rect,
    /// Position block: present while there is a position (current or last known).
    pub pos_label: Option<Rect>,
    pub pos_card: Option<Rect>,
    pub copy_coords: Option<Rect>,
    pub copy_link: Option<Rect>,
    pub sat_label: Rect,
    pub sat_card: Rect,
    pub note: Rect,
    pub total: i32,
}

pub fn layout(frame: &Frame, st: &Option<Status>) -> GpsLayout {
    let c = frame.padded();
    let has_pos = gps(st).is_some_and(|g| g.lat.is_some() && g.lon.is_some());
    let mut y = SP_4;
    let back = Rect::new(c.x, y, TOUCH_MIN, BUTTON_H);
    let chip = Rect::new(c.right() - 140, y + (BUTTON_H - 32) / 2, 140, 32);
    let tx = back.right() + SP_2;
    let title = Rect::new(
        tx,
        y + (BUTTON_H - TITLE_H) / 2,
        (chip.x - SP_3 - tx).max(80),
        TITLE_H,
    );
    y += BUTTON_H + SP_3;
    let state_card = Rect::new(c.x, y, c.w, STATE_CARD_H);
    y += STATE_CARD_H + SP_3;
    let switch_row = Rect::new(c.x, y, c.w, ROW_H);
    y += ROW_H + SP_4;
    let (mut pos_label, mut pos_card, mut copy_coords, mut copy_link) = (None, None, None, None);
    if has_pos {
        pos_label = Some(Rect::new(c.x, y, c.w, LABEL_H));
        y += LABEL_H + SP_2;
        let card = Rect::new(c.x, y, c.w, POS_CARD_H);
        // two buttons side by side inside the card, 56 px high
        let bw = (card.w - 2 * SP_4 - SP_3) / 2;
        let by = card.bottom() - SP_4 - BUTTON_H;
        copy_coords = Some(Rect::new(card.x + SP_4, by, bw, BUTTON_H));
        copy_link = Some(Rect::new(card.x + SP_4 + bw + SP_3, by, bw, BUTTON_H));
        pos_card = Some(card);
        y += POS_CARD_H + SP_4;
    }
    let sat_label = Rect::new(c.x, y, c.w, LABEL_H);
    y += LABEL_H + SP_2;
    let sat_card = Rect::new(c.x, y, c.w, SAT_CARD_H);
    y += SAT_CARD_H + SP_3;
    let note = Rect::new(c.x + SP_1, y, c.w - SP_1, NOTE_H);
    y += NOTE_H;
    GpsLayout {
        back,
        title,
        chip,
        state_card,
        switch_row,
        pos_label,
        pos_card,
        copy_coords,
        copy_link,
        sat_label,
        sat_card,
        note,
        total: y + SP_6,
    }
}

/// Everything the screen draws from.
pub struct View<'a> {
    pub st: &'a Option<Status>,
    pub online: bool,
    pub pressed: Option<Target>,
}

/// Bar height for a signal of `snr` dB-Hz in a chart `h` px high (0 = a stub: tracked, no signal).
pub fn bar_height(snr: u8, h: i32) -> i32 {
    let f = (snr.min(50) as f32) / 50.0;
    ((f * h as f32).round() as i32).max(4)
}

pub fn render(p: &mut Painter, frame: &Frame, v: &View) -> Vec<(Rect, Target)> {
    let l = layout(frame, v.st);
    let mut hits = Vec::new();
    let pr = |t: Target| v.pressed == Some(t);
    let g = gps(v.st);
    p.outline_box(l.back, 0, pr(Target::Back), false, false);
    p.cv.icon(
        Icon::Back,
        l.back.x + (l.back.w - ICON) / 2,
        l.back.y + (l.back.h - ICON) / 2,
        ICON,
        text(),
    );
    hits.push((l.back, Target::Back));
    p.screen_title("GPS", l.title);
    let (chip, chip_col) = chip(v.st);
    p.outline_box(l.chip, 0, false, false, false);
    p.cv.rect(Rect::new(l.chip.x, l.chip.y, 3, l.chip.h), chip_col);
    p.cv.rect(Rect::new(l.chip.x + 12, l.chip.y + 12, 8, 8), chip_col);
    p.text(
        chip,
        Style::new(Face::Regular, SIZE_SECONDARY, text()),
        Rect::new(l.chip.x + 28, l.chip.y, l.chip.w - 36, l.chip.h),
        Align::Left,
    );

    // state card
    let card = l.state_card;
    p.card(card, false);
    p.cv.icon(Icon::Location, card.x + SP_4, card.y + SP_4, 28, text());
    let (title, sub, sub_col) = state_lines(v.st);
    let tx = card.x + SP_4 + 28 + SP_4;
    p.text(
        &title,
        Style::new(Face::Bold, SIZE_BODY, text()),
        Rect::new(tx, card.y + SP_4, card.right() - tx - SP_4, 28),
        Align::Left,
    );
    p.text(
        &sub,
        Style::new(Face::Regular, SIZE_SECONDARY, sub_col),
        Rect::new(tx, card.y + SP_4 + 32, card.right() - tx - SP_4, 44),
        Align::Left,
    );

    // switch
    p.row_group(l.switch_row);
    let dis = !v.online || !switchable(v.st);
    let ssub = if switch_on(v.st) { "On" } else { "Off" };
    if let Some(t) = p.list_row(
        l.switch_row,
        Some(Icon::Location),
        "GPS",
        Some(ssub),
        &Accessory::Toggle(switch_on(v.st)),
        pr(Target::GpsSwitch),
        dis,
    ) {
        hits.push((switch_hit(t), Target::GpsSwitch));
    }
    hits.push((l.switch_row, Target::GpsSwitch));

    // position
    if let (Some(lab), Some(pc), Some(g)) = (l.pos_label, l.pos_card, g) {
        p.section_label("Position", lab);
        p.card(pc, false);
        let coords = coords_text(g).unwrap_or_default();
        p.text(
            &coords,
            Style::new(Face::Mono, 22.0, text()),
            Rect::new(pc.x + SP_4, pc.y + SP_3, pc.w - 2 * SP_4, 32),
            Align::Left,
        );
        let mut parts: Vec<String> = Vec::new();
        if let Some(a) = g.accuracy_m {
            parts.push(format!("Accuracy {a} m"));
        }
        if let Some(a) = g.alt_m {
            parts.push(format!("Altitude {a} m"));
        }
        if !parts.is_empty() {
            p.text(
                &parts.join("  ·  "),
                Style::new(Face::Regular, SIZE_SECONDARY, text_secondary()),
                Rect::new(pc.x + SP_4, pc.y + 48, pc.w - 2 * SP_4, 22),
                Align::Left,
            );
        }
        if let Some(age) = g.age_s {
            let fresh = g.state == GpsState::Fix && age <= FRESH_S;
            let (text, col) = if fresh {
                (format!("Last fix {}", age_text(age)), success())
            } else {
                (format!("Last known position, {}", age_text(age)), warning())
            };
            p.text(
                &text,
                Style::new(Face::Regular, SIZE_SECONDARY, col),
                Rect::new(pc.x + SP_4, pc.y + 74, pc.w - 2 * SP_4, 22),
                Align::Left,
            );
        }
        if let Some(b) = l.copy_coords {
            let state = if pr(Target::GpsCopyCoords) {
                ButtonState::Pressed
            } else {
                ButtonState::Default
            };
            p.button(b, "Copy position", ButtonKind::Secondary, state, None);
            hits.push((b, Target::GpsCopyCoords));
        }
        if let Some(b) = l.copy_link {
            let state = if pr(Target::GpsOpenMap) {
                ButtonState::Pressed
            } else {
                ButtonState::Default
            };
            p.button(b, "OpenStreetMap", ButtonKind::Primary, state, None);
            hits.push((b, Target::GpsOpenMap));
        }
    }

    // satellites
    p.section_label("Satellites", l.sat_label);
    let sc = l.sat_card;
    p.card(sc, false);
    let signals: &[u8] = g.map(|g| g.signals.as_slice()).unwrap_or(&[]);
    let head = match g.map(|g| (g.satellites_view, g.satellites_used)) {
        Some((Some(v), Some(u))) => format!("{v} in view  ·  {u} used in the fix"),
        Some((Some(v), None)) => format!("{v} in view"),
        _ => "No satellites yet".to_string(),
    };
    p.text(
        &head,
        Style::new(Face::Regular, SIZE_BODY, text()),
        Rect::new(sc.x + SP_4, sc.y + SP_3, sc.w - 2 * SP_4, 28),
        Align::Left,
    );
    // bar chart: up to 16 bars, strongest first, 4 px stubs for satellites without a signal
    let chart = Rect::new(
        sc.x + SP_4,
        sc.y + 48,
        sc.w - 2 * SP_4,
        SAT_CARD_H - 48 - 40,
    );
    let base = chart.bottom();
    p.cv.rect(Rect::new(chart.x, base, chart.w, 1), divider());
    if !signals.is_empty() {
        let n = signals.len().min(16) as i32;
        let gap = 6;
        let bw = ((chart.w - gap * (n - 1)) / n).clamp(8, 40);
        for (i, s) in signals.iter().take(16).enumerate() {
            let h = bar_height(*s, chart.h);
            let x = chart.x + i as i32 * (bw + gap);
            let col = if *s == 0 { divider() } else { text() };
            p.cv.rect(Rect::new(x, base - h, bw, h), col);
        }
    }
    p.text(
        "Signal strength of the strongest satellites (0 to 50 dB-Hz)",
        Style::new(Face::Regular, SIZE_CAPTION, text_secondary()),
        Rect::new(sc.x + SP_4, sc.bottom() - 32, sc.w - 2 * SP_4, 24),
        Align::Left,
    );

    p.text(
        "Needs open sky and Wi-Fi on. The first fix can take minutes.",
        Style::new(Face::Regular, SIZE_CAPTION, text_secondary()),
        l.note,
        Align::Left,
    );
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(g: GpsStatus) -> Option<Status> {
        let mut s = crate::app::tests::status(120, &[]);
        s.gps = g;
        Some(s)
    }

    fn fix() -> GpsStatus {
        GpsStatus {
            enabled: true,
            state: GpsState::Fix,
            lat: Some(23.75123),
            lon: Some(90.40988),
            alt_m: Some(12),
            accuracy_m: Some(7),
            satellites_used: Some(5),
            satellites_view: Some(8),
            age_s: Some(1),
            signals: vec![41, 33, 20, 0],
            ..GpsStatus::default()
        }
    }

    #[test]
    fn states_chip_and_lines() {
        assert_eq!(chip(&None).0, "Unknown");
        assert_eq!(chip(&st(GpsStatus::default())).0, "Off");
        assert_eq!(chip(&st(fix())).0, "Fix");
        let (t, s, c) = state_lines(&st(fix()));
        assert_eq!((t.as_str(), c), ("Position found", SUCCESS));
        assert!(s.contains("Within 7 m") && s.contains("5 of 8"), "{s}");
        let failed = GpsStatus {
            enabled: true,
            state: GpsState::Failed,
            reason: Some("turn Wi-Fi on first".into()),
            ..GpsStatus::default()
        };
        let (t, s, c) = state_lines(&st(failed));
        assert_eq!(
            (t.as_str(), s.as_str(), c),
            ("GPS failed", "turn Wi-Fi on first", DANGER)
        );
        let search = GpsStatus {
            enabled: true,
            state: GpsState::Searching,
            satellites_view: Some(3),
            ..GpsStatus::default()
        };
        assert!(state_lines(&st(search.clone()))
            .1
            .contains("3 satellites in view"));
        assert_eq!(qs_line(&st(search)).0, "Searching, 3 satellites");
        assert_eq!(qs_line(&st(fix())).0, "Fix, within 7 m");
        assert_eq!(qs_line(&st(GpsStatus::default())).0, "Off");
    }

    #[test]
    fn switch_and_busy() {
        assert!(!switchable(&None));
        assert!(switchable(&st(GpsStatus::default())));
        let starting = GpsStatus {
            enabled: true,
            state: GpsState::Starting,
            ..GpsStatus::default()
        };
        assert!(busy(&st(starting.clone())) && !switchable(&st(starting)));
        assert!(switch_on(&st(fix())) && !switch_on(&st(GpsStatus::default())));
    }

    #[test]
    fn pill_mark_only_while_on() {
        assert_eq!(pill_mark(&st(GpsStatus::default())), None);
        assert_eq!(pill_mark(&st(fix())), Some(TEXT));
        let s = GpsStatus {
            enabled: true,
            state: GpsState::Searching,
            ..GpsStatus::default()
        };
        assert_eq!(pill_mark(&st(s)), Some(TEXT_DISABLED));
        let f = GpsStatus {
            state: GpsState::Failed,
            ..GpsStatus::default()
        };
        assert_eq!(pill_mark(&st(f)), None);
        assert_eq!(pill_mark(&None), None);
    }

    #[test]
    fn coordinates_and_link() {
        let g = fix();
        assert_eq!(coords_text(&g).unwrap(), "23.75123, 90.40988");
        assert_eq!(
            map_link(&g).unwrap(),
            "https://www.openstreetmap.org/?mlat=23.75123&mlon=90.40988#map=17/23.75123/90.40988"
        );
        let south_west = GpsStatus {
            lat: Some(-33.8688),
            lon: Some(-70.5),
            ..GpsStatus::default()
        };
        assert_eq!(coords_text(&south_west).unwrap(), "-33.86880, -70.50000");
        assert!(coords_text(&GpsStatus::default()).is_none());
        assert!(map_link(&GpsStatus::default()).is_none());
    }

    #[test]
    fn age_wording() {
        assert_eq!(age_text(0), "just now");
        assert_eq!(age_text(12), "12 s ago");
        assert_eq!(age_text(150), "2 min ago");
        assert_eq!(age_text(4000), "over an hour ago");
    }

    #[test]
    fn bar_heights() {
        assert_eq!(bar_height(0, 100), 4);
        assert_eq!(bar_height(25, 100), 50);
        assert_eq!(bar_height(99, 100), 100);
    }

    #[test]
    fn layout_has_the_position_block_only_with_a_position() {
        let f = Frame::new(800, 1280);
        let none = layout(&f, &st(GpsStatus::default()));
        assert!(none.pos_card.is_none() && none.copy_coords.is_none());
        let with = layout(&f, &st(fix()));
        let (a, b) = (with.copy_coords.unwrap(), with.copy_link.unwrap());
        assert!(a.h >= 56 && b.h >= 56 && a.w >= 56);
        assert!(a.right() < b.x, "buttons do not overlap");
        assert!(with.total > none.total);
        assert!(with.switch_row.h >= 56);
    }
}
