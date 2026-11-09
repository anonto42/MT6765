//! M5.2: one golden per component, every state side by side.
use std::path::Path;
use walpad_ui::components::*;
use walpad_ui::layout::Frame;
use walpad_ui::snapshot::assert_snapshot;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Canvas, Face, Fonts, Icon, Rect, Style};

fn fonts() -> Fonts {
    Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")).unwrap()
}
type RowSpec = (
    Option<Icon>,
    &'static str,
    Option<&'static str>,
    Accessory,
    bool,
    bool,
);

fn label(p: &mut Painter, s: &str, x: i32, y: i32, w: i32) {
    p.text(
        s,
        Style::new(Face::Regular, 12.0, TEXT_DISABLED),
        Rect::new(x, y, w, 16),
        Align::Left,
    );
}
macro_rules! sheet {
    ($f:ident, $c:ident, $p:ident, $w:expr, $h:expr) => {
        let mut $f = fonts();
        let mut $c = Canvas::new($w, $h, BG).unwrap();
        let mut $p = Painter {
            cv: &mut $c,
            fonts: &mut $f,
        };
    };
}

#[test]
fn button_states() {
    sheet!(f, c, p, 800, 390);
    let kinds = [
        ("primary", ButtonKind::Primary),
        ("secondary", ButtonKind::Secondary),
        ("danger", ButtonKind::Danger),
    ];
    let states = [
        ("default", ButtonState::Default),
        ("pressed", ButtonState::Pressed),
        ("disabled", ButtonState::Disabled),
        ("loading", ButtonState::Loading(45)),
    ];
    for (ki, (kn, k)) in kinds.iter().enumerate() {
        for (si, (sn, s)) in states.iter().enumerate() {
            let (x, y) = (24 + si as i32 * 190, 24 + ki as i32 * 92);
            p.button(Rect::new(x, y, 176, BUTTON_H), "Connect", *k, *s, None);
            label(&mut p, &format!("{kn} / {sn}"), x, y + 60, 176);
        }
    }
    p.button(
        Rect::new(24, 304, 200, BUTTON_H),
        "Scan",
        ButtonKind::Secondary,
        ButtonState::Default,
        Some(Icon::Refresh),
    );
    assert_snapshot("comp_buttons", &c);
}

#[test]
fn app_tiles_and_status_cards() {
    sheet!(f, c, p, 800, 640);
    p.app_tile(
        Rect::new(24, 24, 364, 160),
        Icon::Globe,
        "Browser",
        false,
        false,
    );
    p.app_tile(
        Rect::new(412, 24, 364, 160),
        Icon::Terminal,
        "Terminal",
        false,
        true,
    );
    p.app_tile(
        Rect::new(24, 200, 364, 148),
        Icon::Folder,
        "Files (pressed)",
        true,
        false,
    );
    p.app_tile(
        Rect::new(412, 200, 364, 148),
        Icon::Settings,
        "Settings",
        false,
        false,
    );
    p.status_card(
        Rect::new(24, 364, 364, 112),
        "Wi-Fi",
        &CardValue::Value("Home".into()),
        Some("192.168.0.100"),
        false,
    );
    p.status_card(
        Rect::new(412, 364, 364, 112),
        "Battery",
        &CardValue::Estimated("62%".into()),
        None,
        false,
    );
    p.status_card(
        Rect::new(24, 492, 364, 112),
        "Battery",
        &CardValue::Unknown,
        None,
        false,
    );
    p.status_card(
        Rect::new(412, 492, 364, 112),
        "Wi-Fi",
        &CardValue::Error("Restart to retry".into()),
        None,
        true,
    );
    assert_snapshot("comp_tiles_cards", &c);
}

#[test]
fn list_rows() {
    sheet!(f, c, p, 800, 8 * 64 + 48);
    let rows: Vec<RowSpec> = vec![
        (
            Some(Icon::Wifi),
            "Network",
            None,
            Accessory::Chevron,
            false,
            false,
        ),
        (
            Some(Icon::Sun),
            "Brightness",
            Some("Native units 0-255"),
            Accessory::Value("120".into()),
            false,
            false,
        ),
        (
            None,
            "Automatic rotation",
            None,
            Accessory::Toggle(true),
            false,
            false,
        ),
        (
            None,
            "Turn off when cover closes",
            None,
            Accessory::Toggle(false),
            false,
            false,
        ),
        (
            None,
            "Restart shell",
            None,
            Accessory::Action("Restart".into()),
            false,
            false,
        ),
        (
            Some(Icon::Settings),
            "Pressed row",
            None,
            Accessory::Chevron,
            true,
            false,
        ),
        (
            Some(Icon::Keyboard),
            "Disabled row",
            Some("Not available"),
            Accessory::Chevron,
            false,
            true,
        ),
        (
            None,
            "A very long row title that has to be shortened with an ellipsis to fit",
            None,
            Accessory::Value("Value".into()),
            false,
            false,
        ),
    ];
    for (i, (ic, t, sub, acc, pr, dis)) in rows.iter().enumerate() {
        let toggle = p.list_row(
            Rect::new(24, 24 + i as i32 * ROW_H, 752, ROW_H),
            *ic,
            t,
            *sub,
            acc,
            *pr,
            *dis,
        );
        assert_eq!(
            toggle.is_some(),
            matches!(acc, Accessory::Toggle(_)),
            "toggle rect only for toggle rows"
        );
    }
    assert_snapshot("comp_list_rows", &c);
}

#[test]
fn controls() {
    sheet!(f, c, p, 800, 430);
    for (i, (on, pr, dis)) in [
        (false, false, false),
        (true, false, false),
        (true, true, false),
        (false, false, true),
        (true, false, true),
    ]
    .iter()
    .enumerate()
    {
        p.toggle(24 + i as i32 * 80, 24, *on, *pr, *dis);
    }
    label(
        &mut p,
        "toggle: off, on, pressed, disabled off, disabled on",
        24,
        62,
        600,
    );
    let sliders = [
        (0.0, false, false),
        (0.4, false, false),
        (0.4, true, false),
        (1.0, false, false),
        (0.6, false, true),
    ];
    for (i, (v, d, dis)) in sliders.iter().enumerate() {
        let track = p.slider(Rect::new(24, 96 + i as i32 * 64, 752, 56), *v, *d, *dis);
        assert_eq!(track.w, 752 - 32);
    }
    label(
        &mut p,
        "slider: 0, 0.4, dragging, 1.0, disabled",
        24,
        96 + 5 * 64 - 4,
        600,
    );
    let segs = p.segmented(
        Rect::new(24, 96 + 5 * 64 + 20, 752, 56),
        &["Auto", "Portrait", "Landscape", "Flipped"],
        1,
        None,
        false,
    );
    assert_eq!(segs.len(), 4);
    assert_snapshot("comp_controls", &c);
}

#[test]
fn network_rows() {
    sheet!(f, c, p, 800, 5 * 72 + 48);
    let rows = [
        ("Cafe Open", 3, false, NetState::Idle, false),
        ("Home", 4, true, NetState::Idle, false),
        ("Home (connected)", 4, true, NetState::Connected, false),
        ("Neighbour", 1, true, NetState::Connecting(90), false),
        (
            "Pressed network with a long name that gets cut off",
            2,
            true,
            NetState::Idle,
            true,
        ),
    ];
    for (i, (s, bars, sec, st, pr)) in rows.iter().enumerate() {
        p.network_row(
            Rect::new(24, 24 + i as i32 * ROW_WIFI_H, 752, ROW_WIFI_H),
            s,
            *bars,
            *sec,
            *st,
            *pr,
        );
    }
    assert_snapshot("comp_network_rows", &c);
}

#[test]
fn text_fields() {
    sheet!(f, c, p, 800, 520);
    p.text_field(
        Rect::new(24, 24, 752, 56),
        "",
        "Network name",
        false,
        false,
        false,
        None,
    );
    p.text_field(
        Rect::new(24, 96, 752, 56),
        "Home",
        "Network name",
        false,
        false,
        true,
        None,
    );
    let eye = p.text_field(
        Rect::new(24, 168, 752, 56),
        "correct horse",
        "Password",
        true,
        false,
        true,
        None,
    );
    assert!(eye.is_some());
    p.text_field(
        Rect::new(24, 240, 752, 56),
        "correct horse",
        "Password",
        true,
        true,
        true,
        None,
    );
    p.text_field(
        Rect::new(24, 312, 752, 56),
        "wrong password",
        "Password",
        true,
        false,
        false,
        Some("Wrong password"),
    );
    p.text_field(
        Rect::new(24, 400, 752, 56),
        "a very long value that will not fit in the field at all, so the end stays visible",
        "x",
        false,
        false,
        true,
        None,
    );
    assert_snapshot("comp_text_fields", &c);
}

#[test]
fn keyboard_layers() {
    sheet!(f, c, p, 800, 3 * 290 + 10);
    for (i, layer) in [
        KeyboardLayer::Lower,
        KeyboardLayer::Upper,
        KeyboardLayer::Symbols,
    ]
    .iter()
    .enumerate()
    {
        let keys = p.keyboard(
            Rect::new(0, 8 + i as i32 * 290, 800, 280),
            *layer,
            if i == 0 { Some(2) } else { None },
        );
        assert!(keys.len() >= 29);
    }
    assert_snapshot("comp_keyboard", &c);
}

#[test]
fn dialogs_sheet_and_power_menu() {
    sheet!(f, c, p, 800, 1280);
    let screen = Rect::new(0, 0, 800, 1280);
    let content = p.sheet(Rect::new(24, 24, 752, 220), "Home");
    assert!(content.h > 100);
    let (cancel, confirm) = p.dialog(
        Rect::new(0, 200, 800, 560),
        "Forget network?",
        "Home will be removed from the saved networks.",
        "Cancel",
        "Forget",
        "Tap again to forget",
        true,
        false,
    );
    assert!(cancel.right() < confirm.x);
    let (_, confirm2) = p.dialog(
        Rect::new(0, 640, 800, 560),
        "Shut down?",
        "The tablet will power off.",
        "Cancel",
        "Shut down",
        "Tap again to shut down",
        true,
        true,
    );
    assert_eq!(confirm2.w, confirm.w);
    let _ = screen;
    assert_snapshot("comp_dialogs", &c);

    sheet!(f2, c2, p2, 800, 760);
    let items = [
        ("Screen off", false),
        ("Restart", true),
        ("Shut down", true),
        ("Cancel", false),
    ];
    let rects = p2.power_menu(Rect::new(0, 0, 800, 760), &items, 3, None);
    assert_eq!(rects.len(), 4);
    assert_snapshot("comp_power_menu_cancel_focus", &c2);
    sheet!(f3, c3, p3, 800, 760);
    p3.power_menu(Rect::new(0, 0, 800, 760), &items, 2, Some(2));
    assert_snapshot("comp_power_menu_armed", &c3);
}

#[test]
fn notices_and_states() {
    sheet!(f, c, p, 800, 900);
    p.toast(Rect::new(0, 0, 800, 200), "Wi-Fi connected", Tone::Info);
    let mut y = 120;
    p.toast(
        Rect::new(0, y, 800, 200),
        "Wi-Fi disconnected",
        Tone::Warning,
    );
    y += 80;
    p.toast(
        Rect::new(0, y, 800, 200),
        "Browser closed unexpectedly",
        Tone::Danger,
    );
    p.banner(
        Rect::new(0, 330, 800, 48),
        "Battery low - connect the charger",
        Tone::Warning,
    );
    p.banner(
        Rect::new(0, 390, 800, 48),
        "System service not responding",
        Tone::Danger,
    );
    p.banner(Rect::new(0, 450, 800, 48), "Update available", Tone::Info);
    p.spinner(100, 560, 48, 30.0);
    p.spinner(200, 560, 24, 200.0);
    p.empty_state(
        Rect::new(300, 510, 480, 120),
        Icon::Wifi,
        "No networks found",
        "Tap Scan to look again",
    );
    let retry = p.error_state(
        Rect::new(0, 650, 800, 240),
        "Wi-Fi failed",
        "Restart the tablet to retry Wi-Fi",
        "Retry",
    );
    assert!(retry.w > 100);
    assert_snapshot("comp_notices_states", &c);
}

#[test]
fn status_bar_and_bottom_nav_portrait_and_landscape() {
    sheet!(f, c, p, 800, 520);
    let fr = Frame::new(800, 1280);
    let bar = |y| Rect::new(0, y, fr.w, fr.status.h);
    p.status_bar(bar(0), Some("Home"), Some(62), true, false);
    p.status_bar(bar(48), None, None, false, false);
    p.status_bar(
        bar(96),
        Some("A Network With A Rather Long Name"),
        Some(15),
        false,
        true,
    );
    for (i, t) in Tab::ALL.iter().enumerate() {
        let hits = p.bottom_nav(Rect::new(0, 150 + i as i32 * 92, 800, NAV_H), *t);
        assert_eq!(hits.len(), 5);
        assert_eq!(hits.iter().map(|r| r.w).sum::<i32>(), 800);
    }
    assert_snapshot("comp_status_nav_portrait", &c);

    sheet!(f2, c2, p2, 1280, 200);
    let fl = Frame::new(1280, 800);
    p2.status_bar(
        Rect::new(0, 0, 1280, 40),
        Some("Home"),
        Some(88),
        false,
        false,
    );
    let hits = p2.bottom_nav(Rect::new(0, 60, 1280, NAV_H), Tab::Network);
    assert_eq!(hits[0].w, 256);
    assert_eq!(fl.nav.h, NAV_H);
    assert_snapshot("comp_status_nav_landscape", &c2);
}
