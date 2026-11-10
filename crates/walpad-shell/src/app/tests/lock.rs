//! Settings > Security: set / change / remove the screen lock (OpenSpec `lock-screen`).
use super::*;
use walpad_proto::{LockKind, LockStatus};

fn set_lock(app: &mut App, l: LockStatus) {
    let mut s = app.model.status.clone().unwrap();
    s.lock = l;
    app.on_core(&CoreEvent::Status(Box::new(s)), 1500);
    render(app);
}

fn security(app: &mut App) {
    tap(app, Target::Nav(crate::chrome::NAV_SETTINGS));
    tap(app, Target::Page(Page::Security));
}

fn pin_set() -> LockStatus {
    LockStatus {
        configured: true,
        kind: Some(LockKind::Pin),
        locked: false,
        retry_in_s: None,
        auto_lock_s: 0,
    }
}

#[test]
fn setting_a_pin_needs_it_twice_and_sends_a_secret_never_printed() {
    let mut app = online();
    security(&mut app);
    tap(&mut app, Target::Lock(LockAct::SetPin));
    let s = sheet(&app);
    assert_eq!(s.purpose, SheetPurpose::LockNew(LockKind::Pin));
    assert_eq!(s.layer, KeyboardLayer::Symbols, "digits first");
    type_text(&mut app, "12");
    assert!(tap(&mut app, Target::SheetConnect).cmds.is_empty());
    assert!(sheet(&app).error.is_some());
    type_text(&mut app, "34");
    assert!(tap(&mut app, Target::SheetConnect).cmds.is_empty());
    assert_eq!(sheet(&app).purpose, SheetPurpose::LockRepeat(LockKind::Pin));
    type_text(&mut app, "1235");
    assert!(tap(&mut app, Target::SheetConnect).cmds.is_empty());
    assert_eq!(sheet(&app).error.as_deref(), Some("The two entries differ"));
    type_text(&mut app, "1234");
    let out = tap(&mut app, Target::SheetConnect);
    let cmd = Cmd::LockSet {
        kind: LockKind::Pin,
        secret: Secret::new("1234"),
        current: None,
    };
    assert_eq!(out.cmds, vec![cmd.clone()]);
    assert!(!format!("{:?}", out.cmds).contains("1234"), "never printed");
    assert_eq!(
        cmd.request(),
        ("lock_set", json!({"kind": "pin", "secret": "1234"}))
    );
    app.note_sent(50, &out.cmds[0]);
    reply(&mut app, 50, json!({"kind": "pin"}));
    assert!(app.model.overlay.is_none() && app.model.lock_flow.is_none());
    assert_eq!(app.model.toast.as_ref().unwrap().text, "Screen lock set");
}

#[test]
fn changing_asks_for_the_current_secret_and_returns_to_it_when_wrong() {
    let mut app = online();
    set_lock(&mut app, pin_set());
    security(&mut app);
    tap(&mut app, Target::Lock(LockAct::Change));
    assert_eq!(
        sheet(&app).purpose,
        SheetPurpose::LockCurrent(LockKind::Pin)
    );
    type_text(&mut app, "9999");
    tap(&mut app, Target::SheetConnect);
    type_text(&mut app, "5555");
    tap(&mut app, Target::SheetConnect);
    type_text(&mut app, "5555");
    let out = tap(&mut app, Target::SheetConnect);
    assert_eq!(
        out.cmds,
        vec![Cmd::LockSet {
            kind: LockKind::Pin,
            secret: Secret::new("5555"),
            current: Some(Secret::new("9999")),
        }]
    );
    assert_eq!(
        out.cmds[0].request().1["current"],
        json!("9999"),
        "current goes to the core"
    );
    app.note_sent(51, &out.cmds[0]);
    reply_err(&mut app, 51, "refused", "wrong PIN or password");
    let s = sheet(&app);
    assert_eq!(s.purpose, SheetPurpose::LockCurrent(LockKind::Pin));
    assert_eq!(s.error.as_deref(), Some("wrong PIN or password"));
}

#[test]
fn removing_needs_the_current_secret_and_lock_now_and_auto_lock_are_plain_commands() {
    let mut app = online();
    set_lock(&mut app, pin_set());
    security(&mut app);
    tap(&mut app, Target::Lock(LockAct::Remove));
    assert_eq!(sheet(&app).purpose, SheetPurpose::LockRemove(LockKind::Pin));
    type_text(&mut app, "4321");
    let out = tap(&mut app, Target::SheetConnect);
    assert_eq!(out.cmds, vec![Cmd::LockRemove(Secret::new("4321"))]);
    app.note_sent(52, &out.cmds[0]);
    reply_err(&mut app, 52, "refused", "wrong PIN or password");
    let s = sheet(&app);
    assert!(!s.connecting && s.secret.is_empty() && s.error.is_some());
    type_text(&mut app, "4321");
    let out = tap(&mut app, Target::SheetConnect);
    app.note_sent(53, &out.cmds[0]);
    reply(&mut app, 53, json!({}));
    assert!(app.model.overlay.is_none());
    assert_eq!(
        app.model.toast.as_ref().unwrap().text,
        "Screen lock removed"
    );

    assert_eq!(
        tap(&mut app, Target::Lock(LockAct::Now)).cmds,
        vec![Cmd::LockNow]
    );
    assert_eq!(
        tap(&mut app, Target::Lock(LockAct::CycleAuto)).cmds,
        vec![Cmd::LockConfig(30)]
    );
    set_lock(
        &mut app,
        LockStatus {
            auto_lock_s: 1800,
            ..pin_set()
        },
    );
    assert_eq!(
        tap(&mut app, Target::Lock(LockAct::CycleAuto)).cmds,
        vec![Cmd::LockConfig(0)]
    );
}

#[test]
fn the_page_says_the_usb_recovery_shell_is_outside_the_lock() {
    use crate::settings_screen::Row;
    let rows = lock_rows(&online().model.status);
    assert!(rows
        .iter()
        .any(|r| matches!(r, Row::Mono(t) if t.contains("NOT protected"))));
    let mut s = online().model.status.clone().unwrap();
    s.lock = pin_set();
    let rows = lock_rows(&Some(s));
    assert!(rows
        .iter()
        .any(|r| matches!(r, Row::Info(l, v, _) if l == "Screen lock" && v == "PIN")));
}
