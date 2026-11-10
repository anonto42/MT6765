//! Device tab: display, battery, touch and hardware status. Content coordinates.
//! Unavailable hardware is labelled unavailable; nothing fake is shown as a working control.
use crate::app::Target;
use crate::config::DevdSettings;
use walpad_proto::{BatterySource, Status};
use walpad_ui::components::{Painter, Tone};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Icon, Rect, Style};

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

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Title,
    Label(&'static str),
    BrightnessCard,
    RotationRow(usize),
    SegLabel(&'static str),
    DimSeg,
    OffSeg,
    LidRow,
    Info(String, String, Tone),
    Gap,
}

pub fn height(item: &Item) -> i32 {
    match item {
        Item::Title => 40 + SP_2,
        Item::Label(_) => 24 + SP_2,
        Item::BrightnessCard => 112 + SP_3,
        Item::RotationRow(_) => ROW_H,
        Item::SegLabel(_) => 28,
        Item::DimSeg | Item::OffSeg => 56 + SP_3,
        Item::LidRow => ROW_H,
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
    v.extend([
        Item::Gap,
        Item::SegLabel("Dim after"),
        Item::DimSeg,
        Item::SegLabel("Screen off after"),
        Item::OffSeg,
        Item::LidRow,
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
    for (l, (val, tone)) in [
        ("Level", level),
        ("State", state),
        ("Voltage", volt),
        ("Temperature", temp),
    ] {
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
        Tone::Info => TEXT_SECONDARY,
        Tone::Warning => WARNING,
        Tone::Danger => DANGER,
    }
}

/// A read-only row: label on the left, value on the right (coloured when it needs attention).
pub fn info_row(p: &mut Painter, r: Rect, label: &str, value: &str, tone: Tone) {
    if !p.cv.visible(r) {
        return;
    }
    p.divider(r.x + SP_4, r.bottom() - 1, r.w - 2 * SP_4);
    let vw = (p.fonts.measure(value, Face::Regular, SIZE_BODY) as i32 + SP_4).min(r.w * 2 / 3);
    p.text(
        label,
        Style::new(Face::Regular, SIZE_BODY, TEXT),
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

fn selected_rotation(st: &Option<Status>) -> Option<usize> {
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

#[allow(clippy::too_many_arguments)]
pub fn render(
    p: &mut Painter,
    frame: &Frame,
    st: &Option<Status>,
    d: &DevdSettings,
    online: bool,
    brightness: f32,
    dragging: bool,
    pressed: Option<Target>,
) -> Vec<(Rect, Target)> {
    let (placed, _) = layout(frame, st, d);
    let mut hits = Vec::new();
    let pr = |t: Target| pressed == Some(t);
    let rot = selected_rotation(st);
    for Placed { item, rect: r } in &placed {
        let r = *r;
        match item {
            Item::Title => p.screen_title("Device", Rect::new(r.x, r.y, r.w, 40)),
            Item::Label(l) => p.section_label(l, Rect::new(r.x, r.y, r.w, 24)),
            Item::SegLabel(l) => p.text(
                l,
                Style::new(Face::Bold, SIZE_SECONDARY, TEXT_SECONDARY),
                Rect::new(r.x + 4, r.y, r.w, 28),
                Align::Left,
            ),
            Item::Gap => {}
            Item::BrightnessCard => {
                let card = Rect::new(r.x, r.y, r.w, 112);
                p.card(card, false);
                let inner = card.inset(SP_4);
                p.text(
                    "Brightness",
                    Style::new(Face::Regular, SIZE_BODY, TEXT),
                    Rect::new(inner.x, inner.y, inner.w, 24),
                    Align::Left,
                );
                let pct = if st.is_some() || dragging {
                    format!("{}%", (brightness * 100.0).round() as i32)
                } else {
                    "--".into()
                };
                p.text(
                    &pct,
                    Style::new(Face::Regular, SIZE_BODY, TEXT_SECONDARY),
                    Rect::new(inner.x, inner.y, inner.w, 24),
                    Align::Right,
                );
                let slider = Rect::new(card.x, inner.y + 28, card.w, 56);
                p.slider(slider, brightness, dragging, !online);
                hits.push((slider, Target::Slider));
            }
            Item::RotationRow(i) => {
                let (label, _) = ROTATION_ROWS[*i];
                if pr(Target::Rot(*i)) && online {
                    p.row_pressed(r);
                }
                p.divider(r.x + SP_4, r.bottom() - 1, r.w - 2 * SP_4);
                let on = rot == Some(*i);
                p.text(
                    label,
                    Style::new(
                        if on { Face::Bold } else { Face::Regular },
                        SIZE_BODY,
                        if online { TEXT } else { TEXT_DISABLED },
                    ),
                    Rect::new(r.x + SP_4, r.y, r.w - 2 * SP_4 - ICON, r.h),
                    Align::Left,
                );
                if on {
                    p.cv.icon(
                        Icon::Check,
                        r.right() - SP_4 - ICON,
                        r.y + (r.h - ICON) / 2,
                        ICON,
                        ACCENT,
                    );
                }
                hits.push((r, Target::Rot(*i)));
            }
            Item::DimSeg | Item::OffSeg => {
                let is_dim = *item == Item::DimSeg;
                let (opts, cur): (&[(u32, &str)], u32) = if is_dim {
                    (&DIM_OPTIONS, d.dim_after_s)
                } else {
                    (&OFF_OPTIONS, d.off_after_s)
                };
                let labels: Vec<&str> = opts.iter().map(|(_, l)| *l).collect();
                let seg = Rect::new(r.x, r.y, r.w, 56);
                let sel = selected_option(opts, cur).unwrap_or(usize::MAX);
                let rects = p.segmented(seg, &labels, sel, None, !online);
                for (i, sr) in rects.into_iter().enumerate() {
                    hits.push((
                        sr,
                        if is_dim {
                            Target::Dim(i)
                        } else {
                            Target::Off(i)
                        },
                    ));
                }
            }
            Item::LidRow => {
                let toggle = p.list_row(
                    r,
                    None,
                    "Turn off when cover closes",
                    None,
                    &walpad_ui::components::Accessory::Toggle(d.lid_off),
                    pr(Target::LidToggle),
                    !online,
                );
                if toggle.is_some() {
                    hits.push((r, Target::LidToggle));
                }
            }
            Item::Info(l, v, t) => info_row(p, r, l, v, *t),
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_and_controls_meet_the_touch_minimums_and_do_not_overlap() {
        for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
            let (placed, total) = layout(&f, &None, &DevdSettings::default());
            let mut last = 0;
            for Placed { item, rect } in &placed {
                assert!(rect.y >= last, "{item:?} overlaps the previous item");
                last = rect.bottom();
                if matches!(
                    item,
                    Item::RotationRow(_)
                        | Item::LidRow
                        | Item::DimSeg
                        | Item::OffSeg
                        | Item::Info(..)
                ) {
                    assert!(rect.h >= TOUCH_MIN, "{item:?} is {} px high", rect.h);
                }
                assert!(rect.right() <= f.w);
            }
            assert!(total > last, "content ends after the last item");
            assert!(total > f.content.h, "the Device tab scrolls");
        }
    }

    #[test]
    fn unavailable_hardware_is_labelled_not_hidden() {
        let st = crate::app::tests::status(120, &[]);
        let it = items(&Some(st), &DevdSettings::default());
        let find = |name: &str| {
            it.iter().find_map(|i| match i {
                Item::Info(l, v, t) if l == name => Some((v.clone(), *t)),
                _ => None,
            })
        };
        assert_eq!(find("Audio"), Some(("Unverified".into(), Tone::Warning)));
        assert_eq!(
            find("Bluetooth"),
            Some(("Unavailable".into(), Tone::Danger))
        );
        assert_eq!(find("SD card"), Some(("Unavailable".into(), Tone::Danger)));
        assert_eq!(find("Camera"), Some(("Unsupported".into(), Tone::Danger)));
        assert_eq!(find("USB OTG"), Some(("Not tested".into(), Tone::Warning)));
        assert_eq!(find("Accelerometer"), Some(("Working".into(), Tone::Info)));
        assert_eq!(
            find("Calibration"),
            Some(("Not available yet".into(), Tone::Warning))
        );
        assert_eq!(
            find("Level"),
            Some(("~62% (estimated from voltage)".into(), Tone::Warning)),
            "the estimate is labelled as one"
        );
        assert_eq!(find("Voltage"), Some(("3.92 V".into(), Tone::Info)));
        assert_eq!(find("Temperature"), Some(("34.0 °C".into(), Tone::Info)));
        assert_eq!(
            find("Low battery below"),
            Some(("3550 mV".into(), Tone::Info))
        );
        // offline: everything is unknown, nothing is invented
        let off = items(&None, &DevdSettings::default());
        assert!(off
            .iter()
            .any(|i| matches!(i, Item::Info(l, v, _) if l == "Level" && v == "Unknown")));
        assert!(off
            .iter()
            .any(|i| matches!(i, Item::Info(l, v, _) if l == "Accelerometer" && v == "Unknown")));
    }

    #[test]
    fn rotation_selection_follows_status() {
        let mut st = crate::app::tests::status(120, &[]);
        assert_eq!(
            selected_rotation(&Some(st.clone())),
            Some(0),
            "unlocked = Auto"
        );
        st.rotation_locked = true;
        for (o, want) in [(0, 1), (90, 2), (180, 3), (270, 4)] {
            st.orientation = o;
            assert_eq!(selected_rotation(&Some(st.clone())), Some(want));
        }
        assert_eq!(selected_rotation(&None), None);
        assert_eq!(selected_option(&OFF_OPTIONS, 240), Some(4));
        assert_eq!(selected_option(&OFF_OPTIONS, 0), Some(6), "Never");
        assert_eq!(
            selected_option(&DIM_OPTIONS, 77),
            None,
            "a custom value selects nothing"
        );
    }
}
