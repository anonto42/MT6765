//! Volume (owner request 2026-11-10): Quick Settings slider and mute, the key pill, Settings > Sound.
use super::*;
use crate::settings_screen;
use walpad_proto::AudioStatus;

fn with_audio(app: &mut App, volume: u8, muted: bool, available: bool) {
    let mut s = app.model.status.clone().unwrap();
    s.audio = AudioStatus {
        volume,
        muted,
        available,
    };
    app.on_core(&CoreEvent::Status(Box::new(s)), 1500);
    render(app);
}

fn track(app: &App, t: Target) -> Rect {
    app.overlay_hits
        .iter()
        .chain(app.content_hits.iter())
        .find(|(_, x)| *x == t)
        .map(|(r, _)| r.inset(SP_4))
        .unwrap()
}

#[test]
fn requests_use_the_volume_protocol() {
    assert_eq!(
        Cmd::SetVolume(40).request(),
        ("volume_set", json!({"percent": 40}))
    );
    assert_eq!(Cmd::Mute(true).request(), ("mute", json!({"on": true})));
    assert_eq!(Cmd::TestTone.request(), ("volume_test", Value::Null));
}

#[test]
fn quick_settings_volume_slider_sets_a_percentage() {
    let mut app = online();
    with_audio(&mut app, 60, false, true);
    quick(&mut app);
    let tr = track(&app, Target::VolumeSlider);
    let (_, cy) = centre(&app, Target::VolumeSlider);
    app.on_gesture(
        &Gesture::Pressed {
            x: tr.right(),
            y: cy,
        },
        0,
    );
    let out = app.on_gesture(
        &Gesture::Tap {
            x: tr.right(),
            y: cy,
        },
        5,
    );
    assert_eq!(out.cmds, vec![Cmd::SetVolume(100)]);
    app.on_gesture(&Gesture::Pressed { x: tr.x, y: cy }, 10);
    assert_eq!(
        app.on_gesture(&Gesture::Tap { x: tr.x, y: cy }, 15).cmds,
        vec![Cmd::SetVolume(0)],
        "0 is allowed for the volume (unlike the brightness)"
    );
    // a drag is rate limited and ends with the final value; brightness is not touched
    let (cx, cy) = centre(&app, Target::VolumeSlider);
    app.on_gesture(&Gesture::Pressed { x: cx, y: cy }, 1000);
    assert!(app.model.slider_active && app.model.slider_target == Target::VolumeSlider);
    assert_eq!(
        app.model.brightness_value(),
        120.0 / 255.0,
        "brightness unaffected"
    );
    app.on_gesture(&Gesture::Cancel, 1010);
    app.on_gesture(&Gesture::DragStart { x: cx, y: cy }, 1010);
    let a = app.on_gesture(&Gesture::Drag { dx: 20, dy: 0 }, 1100);
    assert!(matches!(a.cmds[..], [Cmd::SetVolume(_)]), "{:?}", a.cmds);
    let b = app.on_gesture(&Gesture::Drag { dx: 20, dy: 0 }, 1130);
    assert!(b.cmds.is_empty() && b.dirty);
    let end = app.on_gesture(&Gesture::DragEnd, 1200);
    assert!(
        matches!(end.cmds[..], [Cmd::SetVolume(v)] if v > 50),
        "{:?}",
        end.cmds
    );
}

#[test]
fn mute_button_toggles_and_the_label_says_so() {
    let mut app = online();
    with_audio(&mut app, 60, false, true);
    quick(&mut app);
    assert_eq!(crate::volume_ui::label(&app.model), "60%");
    assert_eq!(
        tap(&mut app, Target::MuteToggle).cmds,
        vec![Cmd::Mute(true)]
    );
    assert_eq!(
        app.model.overlay,
        Some(Overlay::QuickSettings),
        "a quick toggle keeps the dropdown open"
    );
    with_audio(&mut app, 60, true, true);
    assert_eq!(crate::volume_ui::label(&app.model), "Muted");
    assert_eq!(
        tap(&mut app, Target::MuteToggle).cmds,
        vec![Cmd::Mute(false)]
    );
}

#[test]
fn without_a_sound_service_the_controls_do_nothing_and_say_unavailable() {
    let mut app = online();
    quick(&mut app);
    // the default status has available = false: the controls are disabled, taps do nothing
    assert_eq!(crate::volume_ui::label(&app.model), "Unavailable");
    let (x, y) = centre(&app, Target::MuteToggle);
    assert!(tap_at(&mut app, x, y, 2000).cmds.is_empty());
    let (x, y) = centre(&app, Target::VolumeSlider);
    app.on_gesture(&Gesture::Pressed { x, y }, 2100);
    assert!(!app.model.slider_active);
    assert!(app.on_gesture(&Gesture::Tap { x, y }, 2110).cmds.is_empty());
    app.on_core(&CoreEvent::Disconnected, 3000);
    render(&mut app);
    assert_eq!(crate::volume_ui::label(&app.model), "--");
}

fn audio_event(app: &mut App, source: &str, volume: u8, muted: bool, now: Ms) -> Outcome {
    let o = app.on_core(
        &CoreEvent::Audio(
            json!({"volume": volume, "muted": muted, "available": true, "source": source}),
        ),
        now,
    );
    render(app);
    o
}

#[test]
fn the_volume_pill_shows_for_keys_only_and_expires() {
    let mut app = online();
    // a slider change (source ipc) needs no pill
    let o = audio_event(&mut app, "ipc", 30, false, 1000);
    assert!(app.model.osd.is_none() && !o.dirty);
    let o = audio_event(&mut app, "key", 65, false, 2000);
    assert!(o.dirty);
    let osd = app.model.osd.unwrap();
    assert_eq!((osd.volume, osd.muted), (65, false));
    assert_eq!(app.next_deadline(), Some(2000 + crate::volume_ui::OSD_MS));
    // another key press extends it
    audio_event(&mut app, "key", 70, false, 3000);
    assert_eq!(
        app.model.osd.unwrap().until,
        3000 + crate::volume_ui::OSD_MS
    );
    assert!(!app.tick(3000 + crate::volume_ui::OSD_MS - 1));
    assert!(app.tick(3000 + crate::volume_ui::OSD_MS));
    assert!(app.model.osd.is_none());
    render(&mut app);
}

#[test]
fn settings_sound_page_has_slider_mute_test_tone_and_output() {
    let mut app = online();
    with_audio(&mut app, 35, false, true);
    go(&mut app, Tab::Settings);
    tap(&mut app, Target::Page(Page::Sound));
    assert_eq!(app.model.settings_page, Some(Page::Sound));
    let rows = app.settings_rows();
    assert!(matches!(
        &rows[0],
        settings_screen::Row::Slider { pct, value, disabled: false, .. } if pct == "35%" && (*value - 0.35).abs() < 1e-6
    ));
    assert!(matches!(
        rows[1],
        settings_screen::Row::Toggle("Mute", false, Target::MuteToggle)
    ));
    assert_eq!(tap(&mut app, Target::TestTone).cmds, vec![Cmd::TestTone]);
    assert_eq!(
        tap(&mut app, Target::MuteToggle).cmds,
        vec![Cmd::Mute(true)]
    );
    let tr = track(&app, Target::VolumeSlider);
    let (_, cy) = centre(&app, Target::VolumeSlider);
    app.on_gesture(
        &Gesture::Pressed {
            x: tr.x + tr.w / 2,
            y: cy,
        },
        5000,
    );
    let out = app.on_gesture(
        &Gesture::Tap {
            x: tr.x + tr.w / 2,
            y: cy,
        },
        5010,
    );
    assert_eq!(out.cmds, vec![Cmd::SetVolume(50)]);
    // back to the list
    tap(&mut app, Target::Back);
    assert_eq!(app.model.settings_page, None);
}

#[test]
fn the_sound_page_is_disabled_without_a_sound_service() {
    let mut app = online();
    go(&mut app, Tab::Settings);
    tap(&mut app, Target::Page(Page::Sound));
    let rows = app.settings_rows();
    assert!(matches!(
        &rows[0],
        settings_screen::Row::Slider { disabled: true, pct, .. } if pct == "Unavailable"
    ));
    assert!(matches!(
        &rows[3],
        settings_screen::Row::Info(l, v, _) if l == "Sound output" && v == "No sound service"
    ));
}
