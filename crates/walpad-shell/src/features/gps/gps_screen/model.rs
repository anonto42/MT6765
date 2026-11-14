use walpad_proto::{GpsState, GpsStatus, Status};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Color, Rect};

pub(super) const TITLE_H: i32 = 40;
pub(super) const STATE_CARD_H: i32 = 104;
pub(super) const LABEL_H: i32 = 24;
pub(super) const POS_CARD_H: i32 = 176;
pub(super) const SAT_CARD_H: i32 = 208;
pub(super) const NOTE_H: i32 = 72;
/// A position older than this is shown as "last known", not as current.
pub const FRESH_S: u32 = 10;

pub(super) fn gps(st: &Option<Status>) -> Option<&GpsStatus> {
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
    let (mut pos_label, mut pos_card, mut copy_coords) = (None, None, None);
    let copy_link = if has_pos {
        pos_label = Some(Rect::new(c.x, y, c.w, LABEL_H));
        y += LABEL_H + SP_2;
        let card = Rect::new(c.x, y, c.w, POS_CARD_H);
        // two buttons side by side inside the card, 56 px high
        let bw = (card.w - 2 * SP_4 - SP_3) / 2;
        let by = card.bottom() - SP_4 - BUTTON_H;
        copy_coords = Some(Rect::new(card.x + SP_4, by, bw, BUTTON_H));
        let osm = Some(Rect::new(card.x + SP_4 + bw + SP_3, by, bw, BUTTON_H));
        pos_card = Some(card);
        y += POS_CARD_H + SP_4;
        osm
    } else {
        let osm = Some(Rect::new(c.x, y, c.w, BUTTON_H));
        y += BUTTON_H + SP_4;
        osm
    };
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
