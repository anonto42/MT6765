//! Volume UI (owner request 2026-11-10): the Quick Settings volume block, the Settings > Sound page
//! rows and the transient volume pill (OSD) for the hardware keys. Pure helpers over `Model`.
use crate::app::{Model, Ms, Target};
use crate::settings_screen::Row;
use walpad_ui::components::Painter;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Icon, Rect, Style};

/// How long the volume pill stays after the last key press.
pub const OSD_MS: Ms = 1500;
/// Slider row of Settings > Sound: label line + the 56 px slider.
pub const SLIDER_ROW_H: i32 = 88;

/// The pill shown for a hardware volume key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Osd {
    pub volume: u8,
    pub muted: bool,
    pub available: bool,
    pub until: Ms,
}

/// Slider position 0..=1: the dragged value while this slider is dragged, else the core's volume.
pub fn level(m: &Model) -> f32 {
    if m.slider_active && m.slider_target == Target::VolumeSlider {
        m.slider_value
    } else {
        m.status
            .as_ref()
            .map_or(0.0, |s| f32::from(s.audio.volume) / 100.0)
    }
}

pub fn muted(m: &Model) -> bool {
    m.status.as_ref().is_some_and(|s| s.audio.muted)
}

/// Can the volume be changed right now (connected to the core and something to control)?
pub fn usable(m: &Model) -> bool {
    m.connected && m.status.as_ref().is_some_and(|s| s.audio.available)
}

/// "60%", "Muted", "Unavailable" or "--" (no core status).
pub fn label(m: &Model) -> String {
    let Some(s) = &m.status else {
        return "--".into();
    };
    if !s.audio.available {
        "Unavailable".into()
    } else if s.audio.muted {
        "Muted".into()
    } else {
        format!("{}%", (level(m) * 100.0).round() as i32)
    }
}

fn speaker(m: &Model) -> Icon {
    if muted(m) {
        Icon::VolumeOff
    } else {
        Icon::Volume
    }
}

/// Headphone jack route caption (owner request 2026-11-12, change headphone-mic-jack): "Volume"
/// normally; "Headphones" / "Headset mic" while the route is in effect. This kernel build has no
/// live jack-insert detection (docs/45-headphone-mic.md), so it is always the owner's manual
/// Settings override, never a sensed plug-in — the word itself is the whole status row.
pub fn route_caption(m: &Model) -> &'static str {
    match m.status.as_ref().and_then(|s| s.jack.route) {
        Some(walpad_proto::JackKind::Headphones) => "Headphones",
        Some(walpad_proto::JackKind::HeadsetWithMic) => "Headset mic",
        None => "Volume",
    }
}

/// Quick Settings: caption line, slider and mute button (hit areas are appended to `hits`).
pub fn render_qs(
    p: &mut Painter,
    m: &Model,
    caption: Rect,
    slider: Rect,
    mute: Rect,
    hits: &mut Vec<(Rect, Target)>,
) {
    let on = usable(m);
    let lc = if on { text() } else { text_disabled() };
    p.cv.icon(Icon::Volume, caption.x, caption.y, ICON, lc);
    p.text(
        route_caption(m),
        Style::new(Face::Regular, SIZE_SECONDARY + 1.0, lc),
        Rect::new(
            caption.x + ICON + SP_3,
            caption.y,
            caption.w - ICON - SP_3,
            caption.h,
        ),
        Align::Left,
    );
    p.text(
        &label(m),
        Style::new(Face::Regular, SIZE_SECONDARY, text_secondary()),
        caption,
        Align::Right,
    );
    p.slider(
        slider,
        level(m),
        m.slider_active && m.slider_target == Target::VolumeSlider,
        !on,
    );
    hits.push((slider, Target::VolumeSlider));
    // mute toggle: a square surface button with the speaker (crossed out while muted)
    let b = Rect::new(mute.x + 8, mute.y + 8, mute.w - 16, mute.h - 16);
    p.surface_box(
        b,
        0,
        surface(),
        m.pressed == Some(Target::MuteToggle),
        on && muted(m),
        !on,
    );
    p.cv.icon(
        speaker(m),
        b.x + (b.w - ICON) / 2,
        b.y + (b.h - ICON) / 2,
        ICON,
        if on && muted(m) {
            warning()
        } else if on {
            text()
        } else {
            text_disabled()
        },
    );
    hits.push((mute, Target::MuteToggle));
}

/// Settings > Sound.
pub fn sound_rows(m: &Model) -> Vec<Row> {
    let avail = m.status.as_ref().is_some_and(|s| s.audio.available);
    let jack = m.status.as_ref().map(|s| s.jack).unwrap_or_default();
    let route_label = match jack.route {
        Some(walpad_proto::JackKind::Headphones) => "Headphones",
        Some(walpad_proto::JackKind::HeadsetWithMic) => "Headset & mic",
        None => "Speaker",
    };
    vec![
        Row::Slider {
            icon: speaker(m),
            label: "Volume",
            pct: label(m),
            value: level(m),
            active: m.slider_active && m.slider_target == Target::VolumeSlider,
            target: Target::VolumeSlider,
            disabled: !usable(m),
        },
        Row::Toggle("Mute", muted(m), Target::MuteToggle),
        Row::Action {
            label: "Play test tone",
            target: Target::TestTone,
            danger: false,
        },
        Row::Info(
            "Sound output".into(),
            if m.status.is_none() {
                "Unknown".into()
            } else if avail {
                route_label.into()
            } else {
                "No sound service".into()
            },
            if avail || m.status.is_none() {
                walpad_ui::components::Tone::Info
            } else {
                walpad_ui::components::Tone::Warning
            },
        ),
        Row::Toggle(
            "Force headphone output",
            jack.forced.is_some(),
            Target::JackHeadphones,
        ),
        Row::Toggle(
            "Use headset microphone",
            jack.forced == Some(walpad_proto::JackKind::HeadsetWithMic),
            Target::JackHeadsetMic,
        ),
    ]
}

/// The volume bar (OSD): a sharp boxy overlay at the bottom centre of `area`.
pub fn render_osd(p: &mut Painter, area: Rect, o: &Osd) -> Rect {
    let (w, h) = (280, 48);
    let r = Rect::new(area.x + (area.w - w) / 2, area.y + area.h * 72 / 100, w, h);
    let off = o.muted || !o.available;
    p.cv.rect(r, surface_strong().over(bg()));
    p.cv.rect(
        Rect::new(r.x, r.y, 3, r.h),
        if off { text_disabled() } else { text() },
    );
    p.cv.icon(
        if off { Icon::VolumeOff } else { Icon::Volume },
        r.x + SP_4,
        r.y + (h - ICON) / 2,
        ICON,
        if o.available { text() } else { text_disabled() },
    );
    let track = Rect::new(
        r.x + SP_4 + ICON + SP_3,
        r.y + h / 2 - 2,
        w - 2 * SP_4 - ICON - SP_3 - 56,
        4,
    );
    p.cv.rect(track, divider());
    let fw = if off {
        0
    } else {
        (track.w as f32 * f32::from(o.volume) / 100.0) as i32
    };
    if fw > 0 {
        p.cv.rect(Rect::new(track.x, track.y, fw.max(4), 4), text());
    }
    let text = if !o.available {
        "--".to_string()
    } else if o.muted {
        "Mute".to_string()
    } else {
        format!("{}%", o.volume)
    };
    p.text(
        &text,
        Style::new(Face::Regular, SIZE_SECONDARY, text_secondary()),
        Rect::new(
            track.right() + SP_2,
            r.y,
            r.right() - SP_4 - track.right() - SP_2,
            h,
        ),
        Align::Right,
    );
    r
}
