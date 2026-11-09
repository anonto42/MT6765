use super::*;
use crate::core_client::ReplyError;
use std::path::Path;
use walpad_proto::{
    AppState, Battery, BatterySource, Hardware, ScreenState, WifiRadio, WifiStatus,
};
use walpad_ui::components::keyboard_layout;
use walpad_ui::Fonts;

pub fn status(brightness: u32, apps: &[(&str, bool)]) -> Status {
    Status {
        battery: Battery {
            percent: Some(62),
            source: BatterySource::VoltageEstimate,
            state: Some("Discharging".into()),
            mv: Some(3920),
            temp_c: Some(34.0),
            low: false,
        },
        screen: ScreenState::On,
        brightness,
        orientation: 0,
        rotation_locked: false,
        lid_closed: false,
        wifi: WifiStatus {
            radio: WifiRadio::Up,
            state: Some("COMPLETED".into()),
            ssid: Some("Home".into()),
            ip: Some("192.168.0.100".into()),
            signal_dbm: Some(-58),
        },
        apps: apps
            .iter()
            .map(|(a, r)| AppState {
                app: a.to_string(),
                running: *r,
            })
            .collect(),
        hardware: Hardware {
            touch: "ok".into(),
            gsensor: "ok".into(),
            audio: "unverified".into(),
            bluetooth: "unavailable".into(),
            sd_card: "unavailable".into(),
        },
    }
}

pub fn catalog() -> Vec<AppEntry> {
    vec![
        AppEntry {
            id: "browser".into(),
            name: "Browser".into(),
            icon: Icon::Globe,
        },
        AppEntry {
            id: "terminal".into(),
            name: "Terminal".into(),
            icon: Icon::Terminal,
        },
        AppEntry {
            id: "files".into(),
            name: "Files".into(),
            icon: Icon::Folder,
        },
        AppEntry {
            id: "wifi".into(),
            name: "Wi-Fi".into(),
            icon: Icon::Wifi,
        },
    ]
}

pub fn wall() -> WallTime {
    WallTime::new(14, 22, "Monday, 09 November", true)
}

thread_local! {
    // parsing the fonts is slow: once per test thread
    static FONTS: std::cell::RefCell<Fonts> = std::cell::RefCell::new(
        Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts")).unwrap(),
    );
}

fn render(app: &mut App) -> Canvas {
    let mut c = Canvas::new(app.frame.w as u32, app.frame.h as u32, BG).unwrap();
    FONTS.with(|f| {
        app.render(&mut Painter {
            cv: &mut c,
            fonts: &mut f.borrow_mut(),
        })
    });
    c
}

fn online() -> App {
    let mut app = App::new(800, 1280, catalog(), wall());
    app.on_core(
        &CoreEvent::Status(Box::new(status(
            120,
            &[("browser", false), ("terminal", true)],
        ))),
        0,
    );
    render(&mut app);
    app
}

/// Screen coordinates of the centre of a target (content hits are translated by the scroll offset).
fn centre(app: &App, t: Target) -> (i32, i32) {
    if let Some((r, _)) = app.overlay_hits.iter().find(|(_, x)| *x == t) {
        return r.center();
    }
    if app.model.overlay.is_none() {
        if let Some((r, _)) = app.content_hits.iter().find(|(_, x)| *x == t) {
            let (cx, cy) = r.center();
            return (cx, cy + app.view.y - app.scroll);
        }
    }
    app.hits
        .iter()
        .find(|(_, x)| *x == t)
        .map(|(r, _)| r.center())
        .unwrap_or_else(|| panic!("no hit area for {t:?}"))
}

fn tap_at(app: &mut App, x: i32, y: i32, now: Ms) -> Outcome {
    app.on_gesture(&Gesture::Pressed { x, y }, now);
    let out = app.on_gesture(&Gesture::Tap { x, y }, now + 10);
    render(app);
    out
}
fn tap(app: &mut App, t: Target) -> Outcome {
    let (x, y) = centre(app, t);
    tap_at(app, x, y, 1000)
}
fn long_press(app: &mut App, t: Target) {
    let (x, y) = centre(app, t);
    app.on_gesture(&Gesture::Pressed { x, y }, 0);
    app.on_gesture(&Gesture::Cancel, 500);
    app.on_gesture(&Gesture::LongPress { x, y }, 500);
    render(app);
}
fn wifi_event(app: &mut App, v: Value) -> Outcome {
    let o = app.on_core(&CoreEvent::Wifi(v), 2000);
    render(app);
    o
}
fn reply(app: &mut App, id: u64, data: Value) -> Outcome {
    let o = app.on_core(
        &CoreEvent::Reply {
            id,
            ok: true,
            data,
            error: None,
        },
        2000,
    );
    render(app);
    o
}
fn reply_err(app: &mut App, id: u64, code: &str, msg: &str) -> Outcome {
    let o = app.on_core(
        &CoreEvent::Reply {
            id,
            ok: false,
            data: Value::Null,
            error: Some(ReplyError {
                code: code.into(),
                msg: msg.into(),
            }),
        },
        2000,
    );
    render(app);
    o
}
fn go(app: &mut App, tab: Tab) -> Outcome {
    let i = Tab::ALL.iter().position(|t| *t == tab).unwrap();
    tap(app, Target::Nav(i))
}
fn scan_result(app: &mut App) {
    wifi_event(
        app,
        json!({"kind":"scan","networks":[
            {"ssid_hex":"486f6d65","ssid":"Home","signal_dbm":-52,"security":"wpa","freq_mhz":5180},
            {"ssid_hex":"4361666520","ssid":"Cafe Open","signal_dbm":-60,"security":"open","freq_mhz":2437},
            {"ssid_hex":"4e6569","ssid":"Neighbour","signal_dbm":-80,"security":"wpa","freq_mhz":2412}]}),
    );
}
/// Type lower-case text and spaces on the password keyboard by tapping real key rectangles.
fn type_text(app: &mut App, text: &str) {
    for ch in text.chars() {
        let g = overlay::password_geom(&app.frame);
        let layer = match &app.model.overlay {
            Some(Overlay::Password(s)) => s.layer,
            _ => panic!("no password sheet"),
        };
        let keys = keyboard_layout(g.keyboard, layer);
        let want = if ch == ' ' { Key::Space } else { Key::Char(ch) };
        let k = keys
            .iter()
            .find(|k| k.key == want)
            .unwrap_or_else(|| panic!("no key for {ch:?}"));
        let (x, y) = k.rect.center();
        tap_at(app, x, y, 3000);
    }
}
fn sheet(app: &App) -> &PasswordSheet {
    match &app.model.overlay {
        Some(Overlay::Password(s)) => s,
        o => panic!("expected the password sheet, got {o:?}"),
    }
}

// ---------------------------------------------------------------------------------- Home

#[test]
fn home_tiles_are_the_pinned_installed_apps() {
    let app = online();
    let ids: Vec<&str> = app.model.tiles.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(
        ids,
        ["browser", "terminal", "settings"],
        "files is not installed so it is not shown"
    );
    assert!(app.model.tiles[1].running && !app.model.tiles[0].running);
    let all: Vec<&str> = app.model.apps.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(
        all,
        ["browser", "terminal", "settings"],
        "the Wi-Fi app entry is the Network tab, not an app tile"
    );
    let off = App::new(800, 1280, catalog(), wall());
    assert_eq!(
        off.model.tiles.len(),
        1,
        "offline: only the Settings shortcut"
    );
    let custom = App::new(800, 1280, catalog(), wall())
        .with_pinned(vec!["settings".into(), "terminal".into()]);
    assert_eq!(
        custom
            .model
            .tiles
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>(),
        ["settings"],
        "terminal is not installed while offline"
    );
}

#[test]
fn home_taps_launch_apps_or_navigate() {
    let mut app = online();
    assert_eq!(
        tap(&mut app, Target::Tile(0)).cmds,
        vec![Cmd::Launch("browser".into())]
    );
    assert_eq!(
        tap(&mut app, Target::Rotation).cmds,
        vec![Cmd::RotateToggle]
    );
    assert_eq!(
        tap(&mut app, Target::Keyboard).cmds,
        vec![Cmd::KeyboardToggle]
    );
    assert_eq!(tap(&mut app, Target::ScreenOff).cmds, vec![Cmd::ScreenOff]);
    assert!(tap(&mut app, Target::Tile(2)).cmds.is_empty());
    assert_eq!(
        app.active,
        Tab::Settings,
        "the Settings tile switches tab locally"
    );
    go(&mut app, Tab::Home);
    tap(&mut app, Target::WifiCard);
    assert_eq!(app.active, Tab::Network);
    go(&mut app, Tab::Home);
    tap(&mut app, Target::BatteryCard);
    assert_eq!(app.active, Tab::Device);
}

#[test]
fn offline_shows_the_banner_and_ignores_commands() {
    let mut app = App::new(800, 1280, catalog(), wall());
    render(&mut app);
    assert_eq!(
        app.banner(),
        Some(("System service not responding", Tone::Danger))
    );
    assert!(tap(&mut app, Target::Rotation).cmds.is_empty());
    assert!(tap(&mut app, Target::Slider).cmds.is_empty());
    assert_eq!(
        go(&mut app, Tab::Network).cmds,
        vec![],
        "navigation works offline, but nothing is requested from the core"
    );
    assert_eq!(app.active, Tab::Network);
    app.on_core(&CoreEvent::Connected, 0);
    app.on_core(&CoreEvent::Status(Box::new(status(120, &[]))), 0);
    assert_eq!(app.banner(), None);
    assert!(app.on_core(&CoreEvent::Disconnected, 0).dirty);
    assert_eq!(app.model.status, None);
    assert!(app.banner().is_some());
}

#[test]
fn low_battery_banner_until_charging() {
    let mut app = online();
    let mut s = status(120, &[]);
    s.battery.low = true;
    app.on_core(&CoreEvent::Status(Box::new(s.clone())), 0);
    assert_eq!(
        app.banner(),
        Some(("Battery low - connect the charger", Tone::Warning))
    );
    s.battery.state = Some("Charging".into());
    app.on_core(&CoreEvent::Status(Box::new(s)), 0);
    assert_eq!(app.banner(), None);
}

#[test]
fn slider_tap_sets_and_drag_is_rate_limited_with_a_final_value() {
    let mut app = online();
    let (cx, cy) = centre(&app, Target::Slider);
    let track = app
        .hits
        .iter()
        .find(|(_, t)| *t == Target::Slider)
        .map(|(r, _)| r.inset(SP_4))
        .unwrap();
    app.on_gesture(
        &Gesture::Pressed {
            x: track.right(),
            y: cy,
        },
        0,
    );
    assert_eq!(
        app.on_gesture(
            &Gesture::Tap {
                x: track.right(),
                y: cy
            },
            5
        )
        .cmds,
        vec![Cmd::SetBrightness(255)]
    );
    app.on_gesture(&Gesture::Pressed { x: track.x, y: cy }, 10);
    assert_eq!(
        app.on_gesture(&Gesture::Tap { x: track.x, y: cy }, 15).cmds,
        vec![Cmd::SetBrightness(1)],
        "never 0: that would switch the screen off"
    );
    app.on_gesture(&Gesture::Pressed { x: cx, y: cy }, 1000);
    app.on_gesture(&Gesture::Cancel, 1010);
    app.on_gesture(&Gesture::DragStart { x: cx, y: cy }, 1010);
    assert_eq!(
        app.on_gesture(&Gesture::Drag { dx: 20, dy: 0 }, 1100)
            .cmds
            .len(),
        1
    );
    let b = app.on_gesture(&Gesture::Drag { dx: 20, dy: 0 }, 1130);
    assert!(b.cmds.is_empty() && b.dirty, "rate limited but redrawn");
    let end = app.on_gesture(&Gesture::DragEnd, 1200);
    assert!(matches!(end.cmds[..], [Cmd::SetBrightness(v)] if v > 128));
}

// ---------------------------------------------------------------------------------- Apps

#[test]
fn apps_screen_launches_closes_and_pins() {
    let mut app = online();
    go(&mut app, Tab::Apps);
    assert_eq!(
        tap(&mut app, Target::AppTile(0)).cmds,
        vec![Cmd::Launch("browser".into())]
    );

    // long press on the running Terminal: sheet with Close app
    long_press(&mut app, Target::AppTile(1));
    assert!(
        matches!(&app.model.overlay, Some(Overlay::App(s)) if s.id == "terminal" && s.running && s.pinned)
    );
    let out = tap(&mut app, Target::CloseApp);
    assert_eq!(out.cmds, vec![Cmd::CloseApp("terminal".into())]);
    assert!(app.model.overlay.is_none());

    // the Browser is not running: no Close button; unpin it
    long_press(&mut app, Target::AppTile(0));
    assert!(app.overlay_hits.iter().all(|(_, t)| *t != Target::CloseApp));
    let out = tap(&mut app, Target::PinToggle);
    assert_eq!(
        out.cmds,
        vec![Cmd::SetPinned(vec![
            "terminal".into(),
            "files".into(),
            "settings".into()
        ])]
    );
    assert_eq!(
        app.model
            .tiles
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>(),
        ["terminal", "settings"]
    );
    // pin it again
    long_press(&mut app, Target::AppTile(0));
    assert!(matches!(&app.model.overlay, Some(Overlay::App(s)) if !s.pinned));
    tap(&mut app, Target::PinToggle);
    assert_eq!(app.model.pinned.last().map(String::as_str), Some("browser"));
    // cancel does nothing
    long_press(&mut app, Target::AppTile(0));
    assert!(tap(&mut app, Target::SheetCancel).cmds.is_empty());
    assert!(app.model.overlay.is_none());
}

#[test]
fn close_reply_that_closed_nothing_is_reported() {
    let mut app = online();
    app.note_sent(11, &Cmd::CloseApp("terminal".into()));
    reply(&mut app, 11, json!({"closed": false}));
    assert_eq!(
        app.model.toast.as_ref().map(|t| t.text.as_str()),
        Some("Could not close the app")
    );
}

// ---------------------------------------------------------------------------------- Network

#[test]
fn entering_network_asks_for_saved_networks_and_scan_is_single_flight() {
    let mut app = online();
    let out = go(&mut app, Tab::Network);
    assert_eq!(out.cmds, vec![Cmd::WifiListSaved]);
    app.note_sent(20, &Cmd::WifiListSaved);
    reply(
        &mut app,
        20,
        json!([{"id":0,"ssid":"Home","ssid_hex":"486f6d65"},{"id":1,"ssid":"Work","ssid_hex":"576f726b"}]),
    );
    assert_eq!(
        app.model
            .net
            .saved
            .iter()
            .map(|s| s.ssid.as_str())
            .collect::<Vec<_>>(),
        ["Home", "Work"]
    );

    let out = tap(&mut app, Target::Scan);
    assert_eq!(out.cmds, vec![Cmd::WifiScan]);
    assert!(app.model.net.scanning);
    assert!(
        tap(&mut app, Target::Scan).cmds.is_empty(),
        "no second scan while one is running"
    );
    // a busy reply keeps the spinner (the first scan is still running)
    app.note_sent(21, &Cmd::WifiScan);
    reply_err(&mut app, 21, "busy", "another Wi-Fi operation is running");
    assert!(app.model.net.scanning);
    scan_result(&mut app);
    assert!(!app.model.net.scanning && app.model.net.scanned);
    assert_eq!(
        app.model
            .net
            .results
            .iter()
            .map(|n| (n.ssid.as_str(), n.bars, n.secured))
            .collect::<Vec<_>>(),
        [
            ("Home", 4, true),
            ("Cafe Open", 3, false),
            ("Neighbour", 1, true)
        ]
    );
}

#[test]
fn scan_error_is_shown_and_clears_the_spinner() {
    let mut app = online();
    go(&mut app, Tab::Network);
    tap(&mut app, Target::Scan);
    app.note_sent(30, &Cmd::WifiScan);
    reply_err(
        &mut app,
        30,
        "internal",
        "Wi-Fi control is unavailable (wpa_supplicant socket)",
    );
    assert!(!app.model.net.scanning);
    assert_eq!(
        app.model.net.error.as_deref(),
        Some("Wi-Fi control is unavailable")
    );
}

#[test]
fn open_network_connects_without_a_password() {
    let mut app = online();
    go(&mut app, Tab::Network);
    scan_result(&mut app);
    let out = tap(&mut app, Target::Avail(1));
    assert_eq!(
        out.cmds,
        vec![Cmd::WifiConnect {
            ssid_hex: Some("4361666520".into()),
            secret: None,
            saved_id: None
        }]
    );
    assert!(app.model.overlay.is_none(), "no sheet for an open network");
    assert_eq!(app.model.net.connecting.as_deref(), Some("4361666520"));
    assert!(
        tap(&mut app, Target::Avail(2)).cmds.is_empty(),
        "busy connecting: other rows are ignored"
    );
    let out = wifi_event(
        &mut app,
        json!({"kind":"connect","ok":true,"error":null,"ssid_hex":"4361666520","ssid":"Cafe Open"}),
    );
    assert_eq!(out.cmds, vec![Cmd::WifiListSaved]);
    assert_eq!(
        app.model.toast.as_ref().map(|t| (t.text.as_str(), t.tone)),
        Some(("Connected to Cafe Open", Tone::Info))
    );
    assert_eq!(app.model.net.connecting, None);
}

#[test]
fn secured_network_password_flow_with_wrong_password_then_success() {
    let mut app = online();
    // the status says we are connected to "Home"; use another SSID in the list for this flow
    go(&mut app, Tab::Network);
    scan_result(&mut app);
    tap(&mut app, Target::Avail(2)); // Neighbour (secured)
    assert_eq!(sheet(&app).ssid, "Neighbour");

    // Connect is disabled until 8 characters are typed
    type_text(&mut app, "short");
    assert!(tap(&mut app, Target::SheetConnect).cmds.is_empty());
    type_text(&mut app, "er pass");
    assert_eq!(sheet(&app).secret, "shorter pass");
    // show / hide
    assert!(!sheet(&app).shown);
    tap(&mut app, Target::Eye);
    assert!(sheet(&app).shown);
    // backspace
    let g = overlay::password_geom(&app.frame);
    let bs = keyboard_layout(g.keyboard, KeyboardLayer::Lower)
        .iter()
        .find(|k| k.key == Key::Backspace)
        .unwrap()
        .rect
        .center();
    tap_at(&mut app, bs.0, bs.1, 4000);
    assert_eq!(sheet(&app).secret, "shorter pas");

    let out = tap(&mut app, Target::SheetConnect);
    let [cmd @ Cmd::WifiConnect {
        ssid_hex: Some(h),
        secret: Some(s),
        saved_id: None,
    }] = &out.cmds[..]
    else {
        panic!("{:?}", out.cmds)
    };
    assert_eq!((h.as_str(), s.expose()), ("4e6569", "shorter pas"));
    assert!(
        !format!("{cmd:?}").contains("shorter"),
        "the secret never appears in Debug output: {cmd:?}"
    );
    let (name, args) = cmd.request();
    assert_eq!(
        (name, args["secret"].as_str()),
        ("wifi_connect", Some("shorter pas")),
        "it is sent on the control socket"
    );
    assert!(sheet(&app).connecting);

    // wrong password: the sheet stays open with an inline error and the typed text is kept
    wifi_event(
        &mut app,
        json!({"kind":"connect","ok":false,"error":"wrong_password","ssid_hex":"4e6569","ssid":"Neighbour"}),
    );
    assert_eq!(sheet(&app).error.as_deref(), Some("Wrong password"));
    assert!(!sheet(&app).connecting);
    assert_eq!(sheet(&app).secret, "shorter pas");
    // typing clears the error; second attempt succeeds and closes the sheet
    type_text(&mut app, "s");
    assert_eq!(sheet(&app).error, None);
    tap(&mut app, Target::SheetConnect);
    let out = wifi_event(
        &mut app,
        json!({"kind":"connect","ok":true,"error":null,"ssid_hex":"4e6569","ssid":"Neighbour"}),
    );
    assert!(app.model.overlay.is_none());
    assert_eq!(out.cmds, vec![Cmd::WifiListSaved]);
}

#[test]
fn timeout_and_immediate_rejection_are_shown_in_the_sheet() {
    let mut app = online();
    go(&mut app, Tab::Network);
    scan_result(&mut app);
    tap(&mut app, Target::Avail(2));
    type_text(&mut app, "abcdefgh");
    tap(&mut app, Target::SheetConnect);
    wifi_event(
        &mut app,
        json!({"kind":"connect","ok":false,"error":"timeout","ssid_hex":"4e6569","ssid":"Neighbour"}),
    );
    assert_eq!(sheet(&app).error.as_deref(), Some("Timed out after 25 s"));
    tap(&mut app, Target::SheetConnect);
    app.note_sent(
        50,
        &Cmd::WifiConnect {
            ssid_hex: None,
            secret: None,
            saved_id: None,
        },
    );
    reply_err(
        &mut app,
        50,
        "invalid_args",
        "passphrase must be 8-63 characters or a 64-digit hex key",
    );
    assert!(sheet(&app).error.as_deref().unwrap().contains("8-63"));
    // cancel wipes the secret and the connecting marker
    tap(&mut app, Target::SheetCancel);
    assert!(app.model.overlay.is_none() && app.model.net.connecting.is_none());
}

#[test]
fn shift_symbols_and_length_cap() {
    let mut app = online();
    go(&mut app, Tab::Network);
    scan_result(&mut app);
    tap(&mut app, Target::Avail(2));
    let g = overlay::password_geom(&app.frame);
    let find = |layer, key| {
        keyboard_layout(g.keyboard, layer)
            .iter()
            .find(|k| k.key == key)
            .unwrap()
            .rect
            .center()
    };
    let (x, y) = find(KeyboardLayer::Lower, Key::Shift);
    tap_at(&mut app, x, y, 1);
    assert_eq!(sheet(&app).layer, KeyboardLayer::Upper);
    let (x, y) = find(KeyboardLayer::Upper, Key::Char('Q'));
    tap_at(&mut app, x, y, 2);
    assert_eq!(
        (sheet(&app).secret.as_str(), sheet(&app).layer),
        ("Q", KeyboardLayer::Lower),
        "shift applies to one letter"
    );
    // symbols layer
    let (x, y) = keyboard_layout(g.keyboard, KeyboardLayer::Lower)
        .iter()
        .rev()
        .find(|k| k.key == Key::Layer)
        .unwrap()
        .rect
        .center();
    tap_at(&mut app, x, y, 3);
    assert_eq!(sheet(&app).layer, KeyboardLayer::Symbols);
    let (x, y) = find(KeyboardLayer::Symbols, Key::Char('7'));
    tap_at(&mut app, x, y, 4);
    assert_eq!(sheet(&app).secret, "Q7");
    // the field holds at most 64 characters
    for _ in 0..80 {
        tap_at(&mut app, x, y, 5);
    }
    assert_eq!(sheet(&app).secret.len(), 64);
}

#[test]
fn saved_network_rows_connect_by_id_and_forget_asks_first() {
    let mut app = online();
    go(&mut app, Tab::Network);
    app.note_sent(60, &Cmd::WifiListSaved);
    reply(
        &mut app,
        60,
        json!([{"id":0,"ssid":"Home","ssid_hex":"486f6d65"},{"id":7,"ssid":"Work","ssid_hex":"576f726b"}]),
    );
    // Home is the current connection: tapping it does nothing; Work connects with its saved id
    assert!(tap(&mut app, Target::Saved(0)).cmds.is_empty());
    assert_eq!(
        tap(&mut app, Target::Saved(1)).cmds,
        vec![Cmd::WifiConnect {
            ssid_hex: None,
            secret: None,
            saved_id: Some(7)
        }]
    );
    // forget: dialog, cancel, again, confirm
    tap(&mut app, Target::Forget(1));
    assert!(matches!(&app.model.overlay, Some(Overlay::Forget { id: 7, ssid }) if ssid == "Work"));
    assert!(tap(&mut app, Target::DialogCancel).cmds.is_empty());
    assert!(app.model.overlay.is_none());
    tap(&mut app, Target::Forget(1));
    assert_eq!(
        tap(&mut app, Target::DialogConfirm).cmds,
        vec![Cmd::WifiForget(7)]
    );
    app.note_sent(61, &Cmd::WifiForget(7));
    assert_eq!(
        reply(&mut app, 61, json!({})).cmds,
        vec![Cmd::WifiListSaved],
        "the list is refreshed after a forget"
    );
}

#[test]
fn an_available_network_with_stored_credentials_connects_without_the_sheet() {
    let mut app = online();
    go(&mut app, Tab::Network);
    app.note_sent(70, &Cmd::WifiListSaved);
    reply(
        &mut app,
        70,
        json!([{"id":3,"ssid":"Neighbour","ssid_hex":"4e6569"}]),
    );
    scan_result(&mut app);
    let out = tap(&mut app, Target::Avail(2));
    assert_eq!(
        out.cmds,
        vec![Cmd::WifiConnect {
            ssid_hex: None,
            secret: None,
            saved_id: Some(3)
        }]
    );
    assert!(app.model.overlay.is_none());
}

#[test]
fn disconnect_button_and_core_loss_reset_the_transient_state() {
    let mut app = online();
    go(&mut app, Tab::Network);
    assert_eq!(
        tap(&mut app, Target::Disconnect).cmds,
        vec![Cmd::WifiDisconnect]
    );
    scan_result(&mut app);
    tap(&mut app, Target::Avail(2));
    type_text(&mut app, "abcdefgh");
    tap(&mut app, Target::SheetConnect);
    // the sheet is modal: the Scan button behind it has no hit area while it is open
    assert!(app.hit(400, 300).is_none());
    app.on_core(&CoreEvent::Disconnected, 5000);
    assert!(
        !sheet(&app).connecting && app.model.net.connecting.is_none() && !app.model.net.scanning
    );
}

#[test]
fn long_lists_scroll_and_taps_follow_the_scroll_offset() {
    let mut app = online();
    go(&mut app, Tab::Network);
    let nets: Vec<Value> = (0..14).map(|i| json!({"ssid_hex": format!("{:02x}", 0x41 + i), "ssid": format!("Net{i:02}"), "signal_dbm": -50 - i, "security": "open", "freq_mhz": 2412})).collect();
    wifi_event(&mut app, json!({"kind":"scan","networks": nets}));
    assert!(
        app.content_h > app.view.h,
        "content is taller than the view"
    );
    let before = app.scroll;
    // drag up by 600 px: content scrolls down, clamped to the end
    for _ in 0..3 {
        app.on_gesture(&Gesture::Drag { dx: 0, dy: -300 }, 0);
    }
    render(&mut app);
    assert!(app.scroll > before && app.scroll <= app.content_h - app.view.h);
    // scrolling past the end is clamped, scrolling back to the top works
    app.on_gesture(
        &Gesture::Drag {
            dx: 0,
            dy: -100_000,
        },
        0,
    );
    assert_eq!(app.scroll, app.content_h - app.view.h);
    // the last network is now visible and tapping it hits that row, not whatever is at that screen y at scroll 0
    let out = tap(&mut app, Target::Avail(13));
    assert_eq!(
        out.cmds,
        vec![Cmd::WifiConnect {
            ssid_hex: Some("4e".into()),
            secret: None,
            saved_id: None
        }]
    );
    app.on_gesture(&Gesture::Drag { dx: 0, dy: 100_000 }, 0);
    assert_eq!(app.scroll, 0);
    // switching tabs resets the scroll; Home never scrolls
    app.on_gesture(&Gesture::Drag { dx: 0, dy: -300 }, 0);
    go(&mut app, Tab::Home);
    assert_eq!(app.scroll, 0);
    assert!(!app.on_gesture(&Gesture::Drag { dx: 0, dy: -300 }, 0).dirty);
}

// ---------------------------------------------------------------------------------- time

#[test]
fn spinner_animates_only_while_something_runs_and_toasts_expire() {
    let mut app = online();
    assert_eq!(app.next_deadline(), None, "idle: nothing to wake for");
    assert!(!app.tick(1000));
    go(&mut app, Tab::Network);
    tap(&mut app, Target::Scan);
    assert!(app.next_deadline().is_some());
    let d0 = app.model.spinner_deg;
    assert!(app.tick(5000));
    assert_ne!(app.model.spinner_deg, d0);
    assert!(!app.tick(5050), "no faster than 10 fps");
    scan_result(&mut app);
    assert_eq!(app.next_deadline(), None, "scan finished: spinner stopped");

    // a toast stays for 3 s
    app.on_core(&CoreEvent::Wifi(json!({"kind":"disconnected"})), 10_000);
    assert_eq!(app.next_deadline(), Some(13_000));
    assert!(!app.tick(12_999));
    assert!(app.tick(13_000));
    assert!(app.model.toast.is_none());
}

#[test]
fn request_shapes_match_the_core_protocol() {
    assert_eq!(
        Cmd::SetBrightness(77).request(),
        ("set_brightness", json!({"value": 77}))
    );
    assert_eq!(
        Cmd::Launch("browser".into()).request(),
        ("launch", json!({"app": "browser"}))
    );
    assert_eq!(
        Cmd::CloseApp("terminal".into()).request(),
        ("close_app", json!({"app": "terminal"}))
    );
    assert_eq!(
        Cmd::SetPinned(vec!["a".into()]).request(),
        ("set_setting", json!({"key": "pinned_apps", "value": ["a"]}))
    );
    assert_eq!(
        Cmd::WifiForget(4).request(),
        ("wifi_forget", json!({"saved_id": 4}))
    );
    assert_eq!(
        Cmd::WifiConnect {
            ssid_hex: None,
            secret: None,
            saved_id: Some(2)
        }
        .request(),
        ("wifi_connect", json!({"saved_id": 2}))
    );
    assert_eq!(Cmd::WifiScan.request(), ("wifi_scan", Value::Null));
}

#[test]
fn clock_formats() {
    let w = WallTime::new(14, 5, "Monday, 09 November", false);
    assert_eq!((w.hour, w.minute, w.suffix), (2, 5, Some("PM")));
    assert_eq!(WallTime::new(0, 0, "x", false).hour, 12);
    assert_eq!(WallTime::new(12, 30, "x", false).suffix, Some("PM"));
    assert_eq!(WallTime::new(23, 59, "x", true).suffix, None);
    let (_, ms) = WallTime::now(true);
    assert!((1..=60_000).contains(&ms));
}

#[test]
fn catalog_and_settings_files() {
    let root = std::env::temp_dir().join(format!("walpad-app-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("etc/walpad")).unwrap();
    std::fs::write(root.join("etc/walpad/apps.json"), r#"[{"id":"browser","name":"Browser","icon":"globe","exec":["x"]},{"id":"weird","exec":["y"]},{"nope":1}]"#).unwrap();
    let cat = load_catalog(&Root::new(&root));
    assert_eq!(cat.len(), 2);
    assert_eq!(
        (cat[0].icon, cat[1].icon, cat[1].name.as_str()),
        (Icon::Globe, Icon::Apps, "weird")
    );
    assert!(read_clock_24h(&Root::new(&root)), "default 24 h");
    assert_eq!(
        read_pinned(&Root::new(&root)),
        ["browser", "terminal", "files", "settings"]
    );
    std::fs::write(
        root.join("etc/walpad/shell.json"),
        r#"{"clock_24h": false, "pinned_apps": ["terminal"]}"#,
    )
    .unwrap();
    assert!(!read_clock_24h(&Root::new(&root)));
    assert_eq!(read_pinned(&Root::new(&root)), ["terminal"]);
}

// ---------------------------------------------------------------------------------- Device

#[test]
fn device_tab_controls_send_the_right_commands() {
    let mut app = online();
    let out = go(&mut app, Tab::Device);
    assert!(
        out.refresh,
        "opening Device asks the shell to re-read the configuration"
    );
    assert_eq!(
        tap(&mut app, Target::Rot(0)).cmds,
        vec![Cmd::Rotate("auto")]
    );
    assert_eq!(tap(&mut app, Target::Rot(2)).cmds, vec![Cmd::Rotate("90")]);
    assert_eq!(tap(&mut app, Target::Rot(4)).cmds, vec![Cmd::Rotate("270")]);
    assert_eq!(
        tap(&mut app, Target::Dim(1)).cmds,
        vec![Cmd::Setting("dim_after_s", json!(30))]
    );
    assert_eq!(
        tap(&mut app, Target::Off(4)).cmds,
        vec![Cmd::Setting("off_after_s", json!(240))]
    );
    assert_eq!(
        tap(&mut app, Target::Off(6)).cmds,
        vec![Cmd::Setting("off_after_s", json!(0))],
        "Never"
    );
    assert_eq!(
        tap(&mut app, Target::LidToggle).cmds,
        vec![Cmd::Setting("lid_off", json!(false))],
        "lid_off defaults to on, so the toggle turns it off"
    );
    // the brightness slider works here too (it lives in the scrolled content)
    let (_, cy) = centre(&app, Target::Slider);
    let track = app.slider_rect().unwrap().inset(SP_4);
    app.on_gesture(
        &Gesture::Pressed {
            x: track.right(),
            y: cy,
        },
        0,
    );
    assert_eq!(
        app.on_gesture(
            &Gesture::Tap {
                x: track.right(),
                y: cy
            },
            5
        )
        .cmds,
        vec![Cmd::SetBrightness(255)]
    );
}

#[test]
fn settings_replies_trigger_a_reload_and_errors_are_shown() {
    let mut app = online();
    go(&mut app, Tab::Device);
    app.note_sent(80, &Cmd::Setting("dim_after_s", json!(30)));
    assert!(
        reply(&mut app, 80, json!({"key":"dim_after_s","value":30})).refresh,
        "read back what the core really wrote"
    );
    app.note_sent(81, &Cmd::Setting("off_after_s", json!(5)));
    let out = reply_err(
        &mut app,
        81,
        "invalid_args",
        "value must be an integer 0-3600",
    );
    assert!(out.refresh);
    assert_eq!(app.model.toast.as_ref().map(|t| t.tone), Some(Tone::Danger));
    // the shell feeds the files back in
    let d = DevdSettings {
        dim_after_s: 30,
        ..DevdSettings::default()
    };
    assert!(app.set_config(d.clone(), false));
    assert!(
        !app.set_config(d, false),
        "unchanged config does not redraw"
    );
    assert!(!app.model.clock_24h);
}

#[test]
fn device_controls_do_nothing_while_the_core_is_down() {
    let mut app = App::new(800, 1280, catalog(), wall());
    render(&mut app);
    go(&mut app, Tab::Device);
    for t in [
        Target::Rot(1),
        Target::Dim(0),
        Target::Off(0),
        Target::LidToggle,
        Target::Slider,
    ] {
        assert!(tap(&mut app, t).cmds.is_empty(), "{t:?}");
    }
}

// ---------------------------------------------------------------------------------- Settings

#[test]
fn settings_pages_open_and_back_returns_to_the_right_place() {
    let mut app = online();
    go(&mut app, Tab::Settings);
    assert_eq!(app.model.settings_page, None);
    assert!(
        tap(&mut app, Target::Page(Page::System)).refresh,
        "opening a page refreshes the system facts"
    );
    assert_eq!(app.model.settings_page, Some(Page::System));
    tap(&mut app, Target::Back);
    assert_eq!(app.model.settings_page, None);
    // Maintenance -> Logs -> back goes to Maintenance, then to the list
    tap(&mut app, Target::Page(Page::Maintenance));
    tap(&mut app, Target::Page(Page::Logs));
    assert_eq!(app.model.settings_page, Some(Page::Logs));
    tap(&mut app, Target::Back);
    assert_eq!(app.model.settings_page, Some(Page::Maintenance));
    tap(&mut app, Target::Back);
    assert_eq!(app.model.settings_page, None);
    // Display is a shortcut to the Device tab; leaving Settings forgets the open page
    tap(&mut app, Target::Page(Page::Display));
    assert_eq!(app.active, Tab::Device);
}

#[test]
fn leaving_settings_closes_the_subpage() {
    let mut app = online();
    go(&mut app, Tab::Settings);
    tap(&mut app, Target::Page(Page::Security));
    go(&mut app, Tab::Home);
    go(&mut app, Tab::Settings);
    assert_eq!(app.model.settings_page, None);
}

#[test]
fn clock_toggle_restart_shell_and_two_step_power_confirm() {
    let mut app = online();
    go(&mut app, Tab::Settings);
    tap(&mut app, Target::Page(Page::Clock));
    assert_eq!(
        tap(&mut app, Target::ClockToggle).cmds,
        vec![Cmd::Setting("clock_24h", json!(false))]
    );
    tap(&mut app, Target::Back);
    tap(&mut app, Target::Page(Page::Maintenance));
    let out = tap(&mut app, Target::RestartShell);
    assert_eq!(out.cmds, vec![Cmd::RestartShell]);
    assert!(app.model.toast.is_some());

    // Shut down: dialog, first confirm only arms it, second within 3 s powers off
    tap(&mut app, Target::Shutdown);
    assert!(matches!(
        app.model.overlay,
        Some(Overlay::PowerConfirm {
            poweroff: true,
            armed_until: None
        })
    ));
    let (x, y) = centre(&app, Target::DialogConfirm);
    app.now = 10_000;
    let first = tap_at(&mut app, x, y, 10_000);
    assert!(first.cmds.is_empty());
    assert!(
        matches!(
            app.model.overlay,
            Some(Overlay::PowerConfirm {
                armed_until: Some(13_010),
                ..
            })
        ),
        "armed for 3 s"
    );
    let second = tap_at(&mut app, x, y, 12_000);
    assert_eq!(second.cmds, vec![Cmd::Power { poweroff: true }]);
    assert!(app.model.overlay.is_none());
    assert_eq!(
        Cmd::Power { poweroff: true }.request(),
        ("power", json!({"action":"poweroff","confirmed":true}))
    );
    assert_eq!(
        Cmd::Power { poweroff: false }.request().1["action"],
        "reboot"
    );

    // the arming expires: a late second tap only re-arms
    tap(&mut app, Target::Reboot);
    let (x, y) = centre(&app, Target::DialogConfirm);
    tap_at(&mut app, x, y, 20_000);
    app.model.toast = None; // the earlier toast would otherwise be the next deadline
    assert_eq!(app.next_deadline(), Some(23_010));
    assert!(app.tick(23_010), "the button goes back to its normal label");
    assert!(matches!(
        app.model.overlay,
        Some(Overlay::PowerConfirm {
            armed_until: None,
            ..
        })
    ));
    let late = tap_at(&mut app, x, y, 24_000);
    assert!(
        late.cmds.is_empty()
            && matches!(
                app.model.overlay,
                Some(Overlay::PowerConfirm {
                    armed_until: Some(_),
                    ..
                })
            )
    );
    // cancel closes without doing anything
    assert!(tap(&mut app, Target::DialogCancel).cmds.is_empty() && app.model.overlay.is_none());
}
