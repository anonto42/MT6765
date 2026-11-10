//! Copy and paste against a real Xvfb: the clipboard keeper with two X clients, the selection
//! toolbar's buttons typing into the focused app, the terminal variant, the clipboard list.
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_shell::clipboard::{ClipEvent, Keeper};
use walpad_shell::seltools::{bar_rect, list_rect, EditBar, EditOut};
use walpad_shell::shell::{Options, Shell};
use walpad_ui::edit_bar::{
    bar_layout, clip_layout, ClipHit, EditBtn, TERMINAL_BUTTONS, TEXT_BUTTONS,
};
use walpad_ui::Fonts;
use walpad_x::XConn;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    self, AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, InputFocus,
    SelectionNotifyEvent, Window, WindowClass, SELECTION_NOTIFY_EVENT,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;

struct Xvfb(
    Child,
    PathBuf,
    #[allow(dead_code)] std::sync::MutexGuard<'static, ()>,
);
static X_TURN: Mutex<()> = Mutex::new(());
static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
impl Drop for Xvfb {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn xvfb() -> Option<Xvfb> {
    let turn = X_TURN.lock().unwrap_or_else(|e| e.into_inner());
    let n = 900 + (std::process::id() % 40) * 8 + NEXT.fetch_add(1, Ordering::SeqCst) % 8;
    let sock = PathBuf::from(format!("/tmp/.X11-unix/X{n}"));
    let _ = std::fs::remove_file(&sock);
    let c = Command::new("Xvfb")
        .args([
            &format!(":{n}"),
            "-screen",
            "0",
            "1280x1280x24",
            "-nolisten",
            "tcp",
            "-ac",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let t = Instant::now();
    while !sock.exists() && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(50));
    }
    sock.exists().then_some(Xvfb(c, sock, turn))
}

fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(8) {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    panic!("timed out waiting for {what}");
}

fn atom(c: &XConn, name: &str) -> u32 {
    c.conn
        .intern_atom(false, name.as_bytes())
        .unwrap()
        .reply()
        .unwrap()
        .atom
}

fn fonts() -> Fonts {
    Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts")).unwrap()
}

/// A client that owns a selection and answers requests for `text` until dropped.
struct Owner {
    stop: Arc<AtomicBool>,
    h: Option<std::thread::JoinHandle<()>>,
}
impl Owner {
    fn start(sock: &Path, selection: &'static str, text: &str) -> Owner {
        let xc = walpad_x::connect(sock).unwrap();
        let win = xc.conn.generate_id().unwrap();
        xc.conn
            .create_window(
                0,
                win,
                xc.root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new(),
            )
            .unwrap();
        let sel = if selection == "PRIMARY" {
            u32::from(AtomEnum::PRIMARY)
        } else {
            atom(&xc, selection)
        };
        xc.conn
            .set_selection_owner(win, sel, x11rb::CURRENT_TIME)
            .unwrap();
        xc.conn.flush().unwrap();
        let utf8 = atom(&xc, "UTF8_STRING");
        let targets = atom(&xc, "TARGETS");
        let text = text.to_string();
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let h = std::thread::spawn(move || {
            while !s2.load(Ordering::Relaxed) {
                match xc.conn.poll_for_event() {
                    Ok(Some(Event::SelectionRequest(e))) => {
                        let mut ok = false;
                        if e.target == utf8 {
                            xc.conn
                                .change_property8(
                                    xproto::PropMode::REPLACE,
                                    e.requestor,
                                    e.property,
                                    utf8,
                                    text.as_bytes(),
                                )
                                .unwrap();
                            ok = true;
                        } else if e.target == targets {
                            xc.conn
                                .change_property32(
                                    xproto::PropMode::REPLACE,
                                    e.requestor,
                                    e.property,
                                    AtomEnum::ATOM,
                                    &[targets, utf8],
                                )
                                .unwrap();
                            ok = true;
                        }
                        let ev = SelectionNotifyEvent {
                            response_type: SELECTION_NOTIFY_EVENT,
                            sequence: 0,
                            time: e.time,
                            requestor: e.requestor,
                            selection: e.selection,
                            target: e.target,
                            property: if ok { e.property } else { 0 },
                        };
                        xc.conn
                            .send_event(false, e.requestor, EventMask::NO_EVENT, ev)
                            .unwrap();
                        xc.conn.flush().unwrap();
                    }
                    Ok(Some(_)) => {}
                    Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                    Err(_) => break,
                }
            }
        });
        Owner { stop, h: Some(h) }
    }
    /// The app closes: its connection goes away and with it the selection.
    fn close(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.h.take() {
            let _ = h.join();
        }
    }
}

/// Ask whoever owns `selection` for its text (what a pasting app does).
fn paste_from(sock: &Path, selection: &str) -> Option<String> {
    let xc = walpad_x::connect(sock).unwrap();
    let win = xc.conn.generate_id().unwrap();
    xc.conn
        .create_window(
            0,
            win,
            xc.root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new(),
        )
        .unwrap();
    let sel = atom(&xc, selection);
    let utf8 = atom(&xc, "UTF8_STRING");
    let prop = atom(&xc, "PASTE_TEST");
    xc.conn
        .convert_selection(win, sel, utf8, prop, x11rb::CURRENT_TIME)
        .unwrap();
    xc.conn.flush().unwrap();
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(3) {
        if let Some(Event::SelectionNotify(e)) = xc.conn.poll_for_event().unwrap() {
            if e.property == 0 {
                return None;
            }
            let p = xc
                .conn
                .get_property(true, win, prop, AtomEnum::ANY, 0, 100_000)
                .unwrap()
                .reply()
                .unwrap();
            return String::from_utf8(p.value).ok();
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

fn pump(k: &mut Keeper, until: impl Fn(&Keeper, &[ClipEvent]) -> bool) -> Vec<ClipEvent> {
    let t = Instant::now();
    let mut all = Vec::new();
    while t.elapsed() < Duration::from_secs(5) {
        all.extend(k.poll().unwrap());
        if until(k, &all) {
            return all;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    panic!("keeper did not get there: {all:?}");
}

#[test]
fn copied_text_survives_the_source_app_and_a_second_client_pastes_it() {
    let Some(x) = xvfb() else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let mut keeper = Keeper::open(&x.1).unwrap();
    // app A copies
    let a = Owner::start(&x.1, "CLIPBOARD", "héllo wörld, ключ");
    pump(&mut keeper, |k, _| k.history().len() == 1);
    assert_eq!(keeper.history().get(0), Some("héllo wörld, ключ"));
    // app A closes: without the keeper the clipboard would be gone
    a.close();
    std::thread::sleep(Duration::from_millis(100));
    let pasted = std::thread::scope(|s| {
        let h = s.spawn(|| paste_from(&x.1, "CLIPBOARD"));
        // the keeper (the owner now) answers while it is polled
        let t = Instant::now();
        while !h.is_finished() && t.elapsed() < Duration::from_secs(4) {
            keeper.poll().unwrap();
            std::thread::sleep(Duration::from_millis(10));
        }
        h.join().unwrap()
    });
    assert_eq!(pasted.as_deref(), Some("héllo wörld, ключ"));

    // a second app copies something else: both are in the history, newest first
    let b = Owner::start(&x.1, "CLIPBOARD", "second");
    pump(&mut keeper, |k, _| k.history().len() == 2);
    assert_eq!(keeper.history().get(0), Some("second"));
    assert_eq!(keeper.history().get(1), Some("héllo wörld, ключ"));
    b.close();

    // paste an old entry again: it becomes the clipboard and the top of the history
    assert!(keeper.paste(1).unwrap());
    assert_eq!(keeper.history().get(0), Some("héllo wörld, ключ"));
    let pasted = std::thread::scope(|s| {
        let h = s.spawn(|| paste_from(&x.1, "CLIPBOARD"));
        let t = Instant::now();
        while !h.is_finished() && t.elapsed() < Duration::from_secs(4) {
            keeper.poll().unwrap();
            std::thread::sleep(Duration::from_millis(10));
        }
        h.join().unwrap()
    });
    assert_eq!(pasted.as_deref(), Some("héllo wörld, ключ"));

    // clear forgets everything, including what is served
    keeper.clear().unwrap();
    assert!(keeper.history().is_empty());
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(paste_from(&x.1, "CLIPBOARD"), None);
}

#[test]
fn primary_is_only_copied_on_request_and_selecting_is_reported() {
    let Some(x) = xvfb() else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let mut keeper = Keeper::open(&x.1).unwrap();
    let old = Owner::start(&x.1, "CLIPBOARD", "copied earlier");
    pump(&mut keeper, |k, _| k.history().len() == 1);
    old.close();
    // an app selects text: PRIMARY changes owner (the toolbar shows) but the clipboard stays
    let sel = Owner::start(&x.1, "PRIMARY", "selected words");
    let evs = pump(&mut keeper, |_, e| {
        e.contains(&ClipEvent::PrimaryChanged(true))
    });
    assert!(evs.contains(&ClipEvent::PrimaryChanged(true)));
    assert_eq!(keeper.history().len(), 1, "a selection is not a copy");
    // Copy (the terminal's toolbar button): PRIMARY goes into the history and the clipboard
    keeper.copy_primary().unwrap();
    pump(&mut keeper, |k, _| k.history().len() == 2);
    assert_eq!(keeper.history().get(0), Some("selected words"));
    sel.close();
    let pasted = std::thread::scope(|s| {
        let h = s.spawn(|| paste_from(&x.1, "CLIPBOARD"));
        let t = Instant::now();
        while !h.is_finished() && t.elapsed() < Duration::from_secs(4) {
            keeper.poll().unwrap();
            std::thread::sleep(Duration::from_millis(10));
        }
        h.join().unwrap()
    });
    assert_eq!(pasted.as_deref(), Some("selected words"));
}

/// A focused app window recording key presses.
fn focus_app(p: &XConn, class: &[u8]) -> Window {
    let c = &p.conn;
    let app = c.generate_id().unwrap();
    c.create_window(
        0,
        app,
        p.root,
        0,
        0,
        800,
        800,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new()
            .background_pixel(0x0033_6699)
            .event_mask(EventMask::KEY_PRESS),
    )
    .unwrap();
    c.change_property8(
        xproto::PropMode::REPLACE,
        app,
        AtomEnum::WM_CLASS,
        AtomEnum::STRING,
        class,
    )
    .unwrap();
    c.map_window(app).unwrap();
    let a = atom(p, "_NET_ACTIVE_WINDOW");
    c.change_property32(
        xproto::PropMode::REPLACE,
        p.root,
        a,
        AtomEnum::WINDOW,
        &[app],
    )
    .unwrap();
    c.set_input_focus(InputFocus::PARENT, app, x11rb::CURRENT_TIME)
        .unwrap();
    c.flush().unwrap();
    app
}

/// Key presses (keycode, state) the app received within `ms`.
fn keys(p: &XConn, ms: u64) -> Vec<(u8, u16)> {
    let mut out = Vec::new();
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(ms) {
        match p.conn.poll_for_event().unwrap() {
            Some(Event::KeyPress(k)) => out.push((k.detail, u16::from(k.state))),
            Some(_) => {}
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    out
}

fn keycode(p: &XConn, sym: u32) -> u8 {
    let s = p.conn.setup();
    let km = p
        .conn
        .get_keyboard_mapping(s.min_keycode, s.max_keycode - s.min_keycode + 1)
        .unwrap()
        .reply()
        .unwrap();
    let per = km.keysyms_per_keycode as usize;
    km.keysyms
        .chunks(per)
        .position(|c| c.first() == Some(&sym) || c.get(1) == Some(&sym))
        .map(|i| s.min_keycode + i as u8)
        .unwrap()
}

fn click(p: &XConn, x: i16, y: i16) {
    let (c, r) = (&p.conn, p.root);
    c.xtest_fake_input(xproto::MOTION_NOTIFY_EVENT, 0, 0, r, x, y, 0)
        .unwrap();
    c.xtest_fake_input(xproto::BUTTON_PRESS_EVENT, 1, 0, r, 0, 0, 0)
        .unwrap();
    c.xtest_fake_input(xproto::BUTTON_RELEASE_EVENT, 1, 0, r, 0, 0, 0)
        .unwrap();
    c.flush().unwrap();
}

fn focus_of(p: &XConn) -> Window {
    p.conn.get_input_focus().unwrap().reply().unwrap().focus
}

/// Drive the toolbar like the shell loop does until `until` holds.
fn run_bar(e: &mut EditBar, f: &mut Fonts, ms: u64) -> Vec<EditOut> {
    let mut out = Vec::new();
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(ms) {
        out.extend(e.poll(f).unwrap());
        std::thread::sleep(Duration::from_millis(10));
    }
    out
}

#[test]
fn toolbar_buttons_type_the_shortcuts_without_taking_the_focus() {
    let Some(x) = xvfb() else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let p = walpad_x::connect(&x.1).unwrap();
    let mut f = fonts();
    let shell_win = {
        // a stand-in for the shell's window (never active here)
        let w = p.conn.generate_id().unwrap();
        p.conn
            .create_window(
                0,
                w,
                p.root,
                0,
                0,
                10,
                10,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new(),
            )
            .unwrap();
        w
    };
    let mut e = EditBar::open(&x.1, shell_win, (800, 1280)).unwrap();
    assert!(e.can_type());
    let app = focus_app(&p, b"Navigator\0firefox-esr\0");
    p.conn.flush().unwrap();
    e.set_wanted(true, &mut f).unwrap();
    run_bar(&mut e, &mut f, 100);
    assert!(e.bar_visible() && e.server_mapped(e.bar_window()));
    let g = bar_rect((800, 1280), 0, 5);
    assert_eq!(
        p.conn
            .get_geometry(e.bar_window())
            .unwrap()
            .reply()
            .unwrap()
            .y,
        g.y as i16
    );
    let attrs = p
        .conn
        .get_window_attributes(e.bar_window())
        .unwrap()
        .reply()
        .unwrap();
    assert!(attrs.override_redirect);

    // every button types its Ctrl shortcut into the focused app
    let ctrl = keycode(&p, 0xffe3);
    for (b, ch) in [
        (EditBtn::Cut, 'x'),
        (EditBtn::Copy, 'c'),
        (EditBtn::Paste, 'v'),
        (EditBtn::SelectAll, 'a'),
        (EditBtn::Undo, 'z'),
    ] {
        let (r, _) = bar_layout(&TEXT_BUTTONS)
            .into_iter()
            .find(|(_, x)| *x == b)
            .unwrap();
        click(&p, (g.x + r.center().0) as i16, (g.y + r.center().1) as i16);
        let outs = run_bar(&mut e, &mut f, 150);
        assert!(outs.contains(&EditOut::Acted), "{b:?}");
        let got = keys(&p, 150);
        let want = keycode(&p, ch as u32);
        assert!(
            got.contains(&(ctrl, 0)) && got.iter().any(|(k, st)| *k == want && st & 4 != 0),
            "{b:?}: {got:?}"
        );
        assert_eq!(focus_of(&p), app, "the toolbar never takes the focus");
    }
    assert!(e.set_wanted(false, &mut f).is_ok());
    assert!(!e.bar_visible());
}

#[test]
fn a_terminal_gets_only_copy_and_paste_and_paste_is_ctrl_shift_v() {
    let Some(x) = xvfb() else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let p = walpad_x::connect(&x.1).unwrap();
    let mut f = fonts();
    let shell_win = p.conn.generate_id().unwrap();
    p.conn
        .create_window(
            0,
            shell_win,
            p.root,
            0,
            0,
            10,
            10,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new(),
        )
        .unwrap();
    let mut e = EditBar::open(&x.1, shell_win, (800, 1280)).unwrap();
    let _app = focus_app(&p, b"st\0St\0");
    e.set_wanted(true, &mut f).unwrap();
    wait_for("terminal mode", || {
        e.poll(&mut f).unwrap();
        e.terminal_mode()
    });
    // two buttons wide
    let w = p
        .conn
        .get_geometry(e.bar_window())
        .unwrap()
        .reply()
        .unwrap()
        .width;
    assert_eq!(w as i32, bar_rect((800, 1280), 0, 2).w);
    let g = e.bar_geometry();

    // Paste = Ctrl+Shift+V
    let (r, _) = bar_layout(&TERMINAL_BUTTONS)[1];
    click(&p, (g.x + r.center().0) as i16, (g.y + r.center().1) as i16);
    run_bar(&mut e, &mut f, 150);
    let got = keys(&p, 150);
    let v = keycode(&p, 'v' as u32);
    let (ctrl, shift) = (keycode(&p, 0xffe3), keycode(&p, 0xffe1));
    assert!(got.iter().any(|(k, _)| *k == ctrl) && got.iter().any(|(k, _)| *k == shift));
    assert!(
        got.iter()
            .any(|(k, st)| *k == v && st & 4 != 0 && st & 1 != 0),
        "{got:?}"
    );

    // Copy = ask the shell to copy the selection; no key (Ctrl+C would interrupt the program)
    let (r, _) = bar_layout(&TERMINAL_BUTTONS)[0];
    click(&p, (g.x + r.center().0) as i16, (g.y + r.center().1) as i16);
    let outs = run_bar(&mut e, &mut f, 150);
    assert!(outs.contains(&EditOut::CopyPrimary), "{outs:?}");
    assert!(keys(&p, 150).is_empty(), "no key for Copy in a terminal");
}

#[test]
fn clipboard_list_pastes_the_tapped_entry() {
    let Some(x) = xvfb() else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let p = walpad_x::connect(&x.1).unwrap();
    let mut f = fonts();
    let shell_win = p.conn.generate_id().unwrap();
    p.conn
        .create_window(
            0,
            shell_win,
            p.root,
            0,
            0,
            10,
            10,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new(),
        )
        .unwrap();
    let mut e = EditBar::open(&x.1, shell_win, (800, 1280)).unwrap();
    let mut keeper = Keeper::open(&x.1).unwrap();
    for t in ["first", "second", "third"] {
        let o = Owner::start(&x.1, "CLIPBOARD", t);
        let n = keeper.history().len();
        pump(&mut keeper, |k, _| k.history().len() == n + 1);
        o.close();
    }
    let app = focus_app(&p, b"Navigator\0firefox-esr\0");
    let entries: Vec<String> = keeper.history().iter().cloned().collect();
    assert_eq!(entries, ["third", "second", "first"]);
    e.show_list(&entries, &mut f).unwrap();
    run_bar(&mut e, &mut f, 100);
    assert!(e.list_visible() && e.server_mapped(e.list_window()));
    let r = list_rect((800, 1280), 0, 3);
    assert_eq!(
        p.conn
            .get_geometry(e.list_window())
            .unwrap()
            .reply()
            .unwrap()
            .height as i32,
        r.h
    );
    // tap the third row ("first")
    let (row, _) = clip_layout(r.w, 3)
        .into_iter()
        .find(|(_, h)| *h == ClipHit::Entry(2))
        .unwrap();
    click(
        &p,
        (r.x + row.center().0) as i16,
        (r.y + row.center().1) as i16,
    );
    let outs = run_bar(&mut e, &mut f, 200);
    assert_eq!(outs, vec![EditOut::Paste(2)]);
    assert!(!e.list_visible(), "the list closes after a paste");
    // what the shell does with it
    assert!(keeper.paste(2).unwrap());
    e.send_paste().unwrap();
    let got = keys(&p, 200);
    assert!(
        got.iter()
            .any(|(k, st)| *k == keycode(&p, 'v' as u32) && st & 4 != 0),
        "Ctrl+V reached the app: {got:?}"
    );
    assert_eq!(focus_of(&p), app);
    let pasted = std::thread::scope(|s| {
        let h = s.spawn(|| paste_from(&x.1, "CLIPBOARD"));
        let t = Instant::now();
        while !h.is_finished() && t.elapsed() < Duration::from_secs(4) {
            keeper.poll().unwrap();
            std::thread::sleep(Duration::from_millis(10));
        }
        h.join().unwrap()
    });
    assert_eq!(pasted.as_deref(), Some("first"));
}

#[test]
fn the_shell_shows_the_toolbar_for_a_selection_and_hides_it_at_home() {
    let Some(x) = xvfb() else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let root = std::env::temp_dir().join(format!("walpad-edit-shell-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("run/walpad")).unwrap();
    let (sock, stop) = (x.1.clone(), Arc::new(AtomicBool::new(false)));
    let s2 = stop.clone();
    let rootp = root.clone();
    let h = std::thread::spawn(move || {
        let mut sh = Shell::new(Options {
            root: Root::new(rootp),
            x_socket: Some(sock),
            core_socket: None,
            fonts_dir: Some(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts"),
            ),
        })
        .unwrap();
        sh.run(&s2)
    });
    struct Join(
        Arc<AtomicBool>,
        Option<std::thread::JoinHandle<Result<(), String>>>,
    );
    impl Drop for Join {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
            if let Some(h) = self.1.take() {
                let _ = h.join();
            }
        }
    }
    let _j = Join(stop, Some(h));
    wait_for("/run/walpad-ok", || root.join("run/walpad-ok").exists());
    let p = walpad_x::connect(&x.1).unwrap();
    let find = |class: &[u8]| {
        for w in p.conn.query_tree(p.root).unwrap().reply().unwrap().children {
            let c = p
                .conn
                .get_property(false, w, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 64)
                .unwrap()
                .reply()
                .unwrap();
            if c.value.starts_with(class) {
                return Some(w);
            }
        }
        None
    };
    let mapped = |w: Window| {
        p.conn
            .get_window_attributes(w)
            .unwrap()
            .reply()
            .unwrap()
            .map_state
            != xproto::MapState::UNMAPPED
    };
    let bar = find(b"walpad-seltools\0").expect("toolbar window");
    let _list = find(b"walpad-clipboard\0").expect("clipboard list window");
    let shell = find(b"walpad-shell\0").expect("shell window");
    assert!(!mapped(bar), "hidden until something is selected");

    // an app in front; text gets selected in it
    let app = focus_app(&p, b"Navigator\0firefox-esr\0");
    let sel = Owner::start(&x.1, "PRIMARY", "some selected text");
    wait_for("toolbar for the selection", || mapped(bar));
    let g = p.conn.get_geometry(bar).unwrap().reply().unwrap();
    assert_eq!((g.width, g.height), (480, 52));

    // Select all really types Ctrl+A, and the focus stays in the app
    let layout = bar_layout(&TEXT_BUTTONS);
    let (r, _) = layout
        .iter()
        .find(|(_, b)| *b == EditBtn::SelectAll)
        .unwrap();
    click(&p, g.x + r.center().0 as i16, g.y + r.center().1 as i16);
    let got = keys(&p, 400);
    assert!(
        got.iter()
            .any(|(k, st)| *k == keycode(&p, 'a' as u32) && st & 4 != 0),
        "{got:?}"
    );
    assert_eq!(focus_of(&p), app);
    wait_for("toolbar gone after the action", || !mapped(bar));

    // selected again, then Home (the shell becomes the active window): the toolbar goes away
    sel.close();
    let sel2 = Owner::start(&x.1, "PRIMARY", "more text");
    wait_for("toolbar for the second selection", || mapped(bar));
    let a = atom(&p, "_NET_ACTIVE_WINDOW");
    p.conn
        .change_property32(
            xproto::PropMode::REPLACE,
            p.root,
            a,
            AtomEnum::WINDOW,
            &[shell],
        )
        .unwrap();
    p.conn.flush().unwrap();
    wait_for("toolbar hidden at Home", || !mapped(bar));
    sel2.close();
}
