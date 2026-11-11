//! GPS switch, Quick Settings row, GPS screen and the header mark (owner request 2026-11-11).
use super::*;
use walpad_proto::{GpsState, GpsStatus};

fn set_gps(app: &mut App, f: impl FnOnce(&mut GpsStatus)) -> Outcome {
    let mut s = app.model.status.clone().unwrap();
    f(&mut s.gps);
    let o = app.on_core(&CoreEvent::Status(Box::new(s)), 1500);
    render(app);
    o
}

fn fix(g: &mut GpsStatus) {
    g.enabled = true;
    g.state = GpsState::Fix;
    g.lat = Some(23.75123);
    g.lon = Some(90.40988);
    g.alt_m = Some(12);
    g.accuracy_m = Some(7);
    g.satellites_used = Some(5);
    g.satellites_view = Some(8);
    g.age_s = Some(1);
    g.signals = vec![41, 33, 20];
}

fn open_screen(app: &mut App) {
    quick(app);
    tap(app, Target::GpsCard);
    assert_eq!(app.active, Tab::Gps);
    assert!(app.model.overlay.is_none());
}

fn toast_text(app: &App) -> Option<String> {
    app.model.toast.as_ref().map(|t| t.text.clone())
}

#[test]
fn quick_settings_row_has_a_switch_and_opens_the_screen() {
    let mut app = online();
    quick(&mut app);
    let out = tap(&mut app, Target::GpsSwitch);
    assert_eq!(out.cmds, vec![Cmd::Gps(true)]);
    assert_eq!(
        app.model.overlay,
        Some(Overlay::QuickSettings),
        "a quick toggle keeps the dropdown open"
    );
    assert_eq!(Cmd::Gps(true).request(), ("gps", json!({"on": true})));
    set_gps(&mut app, fix);
    quick(&mut app);
    assert_eq!(tap(&mut app, Target::GpsSwitch).cmds, vec![Cmd::Gps(false)]);
    tap(&mut app, Target::GpsCard);
    assert_eq!(app.active, Tab::Gps);
    assert!(app.model.overlay.is_none());
    // the screen has the same switch
    assert_eq!(tap(&mut app, Target::GpsSwitch).cmds, vec![Cmd::Gps(false)]);
}

#[test]
fn the_switch_is_dead_while_starting_and_while_offline() {
    let mut app = online();
    set_gps(&mut app, |g| {
        g.enabled = true;
        g.state = GpsState::Starting;
    });
    quick(&mut app);
    assert!(tap(&mut app, Target::GpsSwitch).cmds.is_empty());
    set_gps(&mut app, |g| g.state = GpsState::Stopping);
    quick(&mut app);
    assert!(tap(&mut app, Target::GpsSwitch).cmds.is_empty());
    // no connection to the core: nothing is sent
    let mut off = online();
    off.on_core(&CoreEvent::Disconnected, 1600);
    render(&mut off);
    quick(&mut off);
    assert!(tap(&mut off, Target::GpsSwitch).cmds.is_empty());
}

#[test]
fn settings_has_a_gps_entry() {
    let mut app = online();
    tap(&mut app, Target::Nav(crate::chrome::NAV_SETTINGS));
    assert_eq!(app.active, Tab::Settings);
    tap(&mut app, Target::Page(Page::Gps));
    assert_eq!(app.active, Tab::Gps);
}

#[test]
fn copy_buttons_put_the_position_and_the_map_link_on_the_clipboard() {
    let mut app = online();
    open_screen(&mut app);
    // no position: no Copy buttons
    assert!(app
        .content_hits
        .iter()
        .all(|(_, t)| !matches!(t, Target::GpsCopyCoords | Target::GpsCopyLink)));
    set_gps(&mut app, fix);
    let out = tap(&mut app, Target::GpsCopyCoords);
    assert_eq!(out.cmds, vec![Cmd::CopyText("23.75123, 90.40988".into())]);
    assert_eq!(toast_text(&app).as_deref(), Some("Position copied"));
    let out = tap(&mut app, Target::GpsCopyLink);
    assert_eq!(
        out.cmds,
        vec![Cmd::CopyText(
            "https://www.openstreetmap.org/?mlat=23.75123&mlon=90.40988#map=17/23.75123/90.40988"
                .into()
        )]
    );
    assert!(toast_text(&app).unwrap().starts_with("Map link copied"));
}

#[test]
fn a_failed_switch_request_is_a_toast() {
    let mut app = online();
    quick(&mut app);
    let out = tap(&mut app, Target::GpsSwitch);
    app.note_sent(61, &out.cmds[0]);
    reply_err(&mut app, 61, "internal", "walpad-gps is not installed");
    assert!(toast_text(&app).is_some());
}

#[test]
fn gps_churn_redraws_only_where_it_is_visible() {
    let mut app = online();
    set_gps(&mut app, fix);
    // on Home: new signal levels and age are invisible: no redraw
    let out = set_gps(&mut app, |g| {
        g.age_s = Some(3);
        g.signals = vec![42, 30, 19];
    });
    assert!(!out.dirty, "no redraw for GPS details nobody sees");
    // a state change is always shown (header mark, Quick Settings row)
    let out = set_gps(&mut app, |g| g.state = GpsState::Searching);
    assert!(out.dirty);
    // the GPS screen shows the details
    open_screen(&mut app);
    let out = set_gps(&mut app, |g| g.age_s = Some(5));
    assert!(out.dirty);
    // and so does an open Quick Settings row
    tap(&mut app, Target::Nav(0));
    quick(&mut app);
    let out = set_gps(&mut app, |g| g.satellites_view = Some(9));
    assert!(out.dirty);
}
