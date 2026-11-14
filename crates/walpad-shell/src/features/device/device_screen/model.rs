use crate::config::DevdSettings;
use walpad_proto::{BatterySource, Status};
use walpad_ui::components::{Painter, Tone};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Color, Face, Rect, Style};

pub const DIM_OPTIONS: [(u32, &str); 5] = [
    (15, "15 s"),
    (30, "30 s"),
    (60, "1 min"),
    (90, "90 s"),
    (0, "Never"),
];
pub const OFF_OPTIONS: [(u32, &str); 7] = [
    (30, "30 s"),
    (60, "1 min"),
    (90, "90 s"),
    (120, "2 min"),
    (240, "4 min"),
    (600, "10 min"),
    (0, "Never"),
];
pub const ROTATION_ROWS: [(&str, &str); 5] = [
    ("Auto", "auto"),
    ("Portrait", "0"),
    ("Landscape", "90"),
    ("Portrait flipped", "180"),
    ("Landscape flipped", "270"),
];
pub const DIM_LEVEL_OPTIONS: [u32; 4] = [10, 20, 40, 60];
pub const LOW_BATT_OPTIONS: [u32; 5] = [3450, 3500, 3550, 3600, 3650];
pub const CRIT_BATT_OPTIONS: [u32; 4] = [3300, 3350, 3400, 3450];

pub fn next_u32(opts: &[u32], cur: u32) -> u32 {
    let idx = opts.iter().position(|&v| v == cur).unwrap_or(0);
    opts[(idx + 1) % opts.len()]
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Title,
    Label(&'static str),
    BrightnessCard,
    RotationRow(usize),
    RotateFlipRow,
    SegLabel(&'static str),
    DimSeg,
    OffSeg,
    LidRow,
    ThemeRow,
    WallpaperSeg,
    WallpaperPathRow,
    Info(String, String, Tone),
    Gap,
}

pub fn height(item: &Item) -> i32 {
    match item {
        Item::Title => 40 + SP_2,
        Item::Label(_) => 24 + SP_2,
        Item::BrightnessCard => 112 + SP_3,
        Item::RotationRow(_) | Item::RotateFlipRow => ROW_H,
        Item::SegLabel(_) => 28,
        Item::DimSeg | Item::OffSeg | Item::WallpaperSeg => 56 + SP_3,
        Item::LidRow | Item::ThemeRow | Item::WallpaperPathRow => ROW_H,
        Item::Info(..) => ROW_H,
        Item::Gap => SP_5,
    }
}

pub fn hw_label(s: &str) -> (&'static str, Tone) {
    match s {
        "ok" => ("Working", Tone::Info),
        "unverified" => ("Unverified", Tone::Warning),
        "unavailable" => ("Unavailable", Tone::Danger),
        "unsupported" => ("Unsupported", Tone::Danger),
        _ => ("Unknown", Tone::Warning),
    }
}

fn unknown() -> (String, Tone) {
    ("Unknown".into(), Tone::Warning)
}

pub fn items(st: &Option<Status>, d: &DevdSettings) -> Vec<Item> {
    let mut v = vec![
        Item::Title,
        Item::Label("Display"),
        Item::BrightnessCard,
        Item::SegLabel("Rotation"),
    ];
    v.extend((0..ROTATION_ROWS.len()).map(Item::RotationRow));
    v.push(Item::RotateFlipRow);
    v.extend([
        Item::Gap,
        Item::SegLabel("Dim after"),
        Item::DimSeg,
        Item::SegLabel("Screen off after"),
        Item::OffSeg,
        Item::LidRow,
        Item::ThemeRow,
        Item::WallpaperPathRow,
        Item::Gap,
        Item::SegLabel("Wallpaper"),
        Item::WallpaperSeg,
        Item::Gap,
    ]);

    v.push(Item::Label("Battery"));
    let b = st.as_ref().map(|s| &s.battery);
    let level = match b {
        Some(b) => match (b.percent, b.source) {
            (Some(p), BatterySource::Gauge) => (format!("{p}%"), Tone::Info),
            (Some(p), BatterySource::VoltageEstimate) => {
                (format!("~{p}% (estimated from voltage)"), Tone::Warning)
            }
            _ => unknown(),
        },
        None => unknown(),
    };
    let state = b
        .and_then(|b| b.state.clone())
        .map(|s| (s, Tone::Info))
        .unwrap_or_else(unknown);
    let volt = b
        .and_then(|b| b.mv)
        .map(|mv| (format!("{:.2} V", mv as f32 / 1000.0), Tone::Info))
        .unwrap_or_else(unknown);
    let temp = b
        .and_then(|b| b.temp_c)
        .map(|t| (format!("{t:.1} °C"), Tone::Info))
        .unwrap_or_else(unknown);
    let mut rows = vec![("Level", level), ("State", state)];
    if let Some(t) = b.and_then(crate::panels::battery_time_text) {
        let label = if b.is_some_and(|b| b.time_left_min.is_none() && b.time_to_full_min.is_some())
        {
            "Time to full"
        } else {
            "Time left"
        };
        rows.push((label, (t, Tone::Info)));
    }
    rows.extend([("Voltage", volt), ("Temperature", temp)]);
    for (l, (val, tone)) in rows {
        v.push(Item::Info(l.into(), val, tone));
    }
    v.push(Item::Info(
        "Low battery below".into(),
        format!("{} mV", d.low_batt_mv),
        Tone::Info,
    ));
    v.push(Item::Info(
        "Shuts down below".into(),
        format!("{} mV", d.critical_batt_mv),
        Tone::Info,
    ));
    v.push(Item::Gap);

    v.push(Item::Label("Touch"));
    let touch = st
        .as_ref()
        .map(|s| hw_label(&s.hardware.touch))
        .unwrap_or(("Unknown", Tone::Warning));
    v.push(Item::Info("Status".into(), touch.0.into(), touch.1));
    v.push(Item::Info(
        "Calibration".into(),
        "Not available yet".into(),
        Tone::Warning,
    ));
    v.push(Item::Gap);

    v.push(Item::Label("Hardware"));
    let hw = |f: fn(&walpad_proto::Hardware) -> &str| {
        st.as_ref()
            .map(|s| hw_label(f(&s.hardware)))
            .unwrap_or(("Unknown", Tone::Warning))
    };
    for (name, (label, tone)) in [
        ("Accelerometer", hw(|h| &h.gsensor)),
        ("Audio", hw(|h| &h.audio)),
        ("Bluetooth", hw(|h| &h.bluetooth)),
        ("SD card", hw(|h| &h.sd_card)),
        ("USB OTG", ("Not tested", Tone::Warning)),
        ("Camera", ("Unsupported", Tone::Danger)),
    ] {
        v.push(Item::Info(name.into(), label.into(), tone));
    }
    v
}

#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub item: Item,
    pub rect: Rect,
}

/// Lay the items out top to bottom; returns them with their rects and the total height.
pub fn layout(frame: &Frame, st: &Option<Status>, d: &DevdSettings) -> (Vec<Placed>, i32) {
    let c = frame.padded();
    let mut y = SP_4;
    let mut out = Vec::new();
    for item in items(st, d) {
        let h = height(&item);
        out.push(Placed {
            rect: Rect::new(c.x, y, c.w, h),
            item,
        });
        y += h;
    }
    (out, y + SP_6)
}

pub fn tone_color(t: Tone) -> Color {
    match t {
        Tone::Info => text_secondary(),
        Tone::Warning => warning(),
        Tone::Danger => danger(),
    }
}

/// A read-only row: label on the left, value on the right (coloured when it needs attention).
pub fn info_row(p: &mut Painter, r: Rect, label: &str, value: &str, tone: Tone) {
    if !p.cv.visible(r) {
        return;
    }
    p.divider(r.x, r.bottom() - 1, r.w);
    let vw = (p.fonts.measure(value, Face::Regular, SIZE_BODY) as i32 + SP_4).min(r.w * 2 / 3);
    p.text(
        label,
        Style::new(Face::Regular, SIZE_BODY, text()),
        Rect::new(r.x + SP_4, r.y, r.w - vw - 2 * SP_4, r.h),
        Align::Left,
    );
    p.text(
        value,
        Style::new(Face::Regular, SIZE_BODY, tone_color(tone)),
        Rect::new(r.right() - vw - SP_4, r.y, vw, r.h),
        Align::Right,
    );
}

pub(super) fn selected_rotation(st: &Option<Status>) -> Option<usize> {
    let s = st.as_ref()?;
    if !s.rotation_locked {
        return Some(0);
    }
    Some(match s.orientation {
        0 => 1,
        90 => 2,
        180 => 3,
        270 => 4,
        _ => return None,
    })
}

pub fn selected_option(options: &[(u32, &str)], value: u32) -> Option<usize> {
    options.iter().position(|(v, _)| *v == value)
}
