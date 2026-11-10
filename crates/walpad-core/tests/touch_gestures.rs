//! Touch gestures against a real Xvfb: recorded touch reports drive the live daemon, test windows
//! (an app in scroll mode, an app in mouse mode, a shell window) see what X would deliver.
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use walpad_core::{daemon::Daemon, fake};
use walpad_hw::evdev::*;
use walpad_hw::Root;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, PropMode, Window, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

static X_TURN: Mutex<()> = Mutex::new(());
static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

struct Xvfb(Child, PathBuf);
impl Drop for Xvfb {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn xvfb(n: u32) -> Option<Xvfb> {
    let child = Command::new("Xvfb")
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
    let sock = PathBuf::from(format!("/tmp/.X11-unix/X{n}"));
    let t = Instant::now();
    while !sock.exists() && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(30));
    }
    sock.exists().then_some(Xvfb(child, sock))
}

fn ev(kind: u16, code: u16, value: i32) -> InputEvent {
    InputEvent {
        sec: 1,
        usec: 0,
        kind,
        code,
        value,
    }
}

struct Running {
    stop: Arc<AtomicBool>,
    h: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Running {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.h.take() {
            let _ = h.join();
        }
    }
}

/// What a test window saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seen {
    Press(u8),
    Release(u8),
    Motion(i16, i16),
}

struct Rig {
    // drop order: the daemon first, then the server, then the turn
    _r: Running,
    _x: Xvfb,
    _turn: std::sync::MutexGuard<'static, ()>,
    root: PathBuf,
    conn: RustConnection,
    wins: Vec<(&'static str, Window)>,
}

impl Rig {
    fn new(name: &str, apps_json: &str) -> Option<Rig> {
        let turn = X_TURN.lock().unwrap_or_else(|e| e.into_inner());
        // a display number of our own: a killed Xvfb leaves its socket behind
        let n = 400 + (std::process::id() % 100) * 8 + NEXT.fetch_add(1, Ordering::SeqCst) % 8;
        let x = xvfb(n)?;
        let root = std::env::temp_dir().join(format!("walpad-tg-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        fake::populate(&root).unwrap();
        std::fs::create_dir_all(root.join("tmp/.X11-unix")).unwrap();
        std::os::unix::fs::symlink(&x.1, root.join("tmp/.X11-unix/X0")).unwrap();
        std::fs::write(
            root.join("etc/walpad/devd.json"),
            r#"{"auto_rotate":false}"#,
        )
        .unwrap();
        std::fs::write(root.join("etc/walpad/apps.json"), apps_json).unwrap();
        let (conn, scr) = x11rb::connect(Some(&format!(":{n}"))).unwrap();
        let rootw = conn.setup().roots[scr].root;
        let mut wins = Vec::new();
        // left column: x 0..400, right column x 400..800; the shell window below y 640
        for (class, x0, y0, w, h) in [
            ("scrollapp", 0i16, 0i16, 400u16, 640u16),
            ("mouseapp", 400, 0, 400, 640),
            ("walpad-testshell", 0, 640, 800, 640),
        ] {
            let win = conn.generate_id().unwrap();
            conn.create_window(
                0,
                win,
                rootw,
                x0,
                y0,
                w,
                h,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new().event_mask(
                    EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
                ),
            )
            .unwrap();
            conn.change_property8(
                PropMode::REPLACE,
                win,
                AtomEnum::WM_CLASS,
                AtomEnum::STRING,
                format!("{class}\0{class}\0").as_bytes(),
            )
            .unwrap();
            conn.map_window(win).unwrap();
            wins.push((class, win));
        }
        conn.flush().unwrap();
        let mut d = Daemon::new(Root::new(&root)).unwrap();
        d.enable_live(true, true);
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let r = Running {
            stop,
            h: Some(std::thread::spawn(move || d.run(&s2))),
        };
        let rig = Rig {
            _r: r,
            _x: x,
            _turn: turn,
            root,
            conn,
            wins,
        };
        // wait for the core's X bridge (the first touch must find it connected)
        std::thread::sleep(Duration::from_millis(800));
        Some(rig)
    }

    fn report(&self, pts: &[(i32, i32)]) {
        let mut evs = Vec::new();
        for (x, y) in pts {
            evs.push(ev(EV_ABS, ABS_MT_POSITION_X, *x));
            evs.push(ev(EV_ABS, ABS_MT_POSITION_Y, *y));
            evs.push(ev(EV_SYN, SYN_MT_REPORT, 0));
        }
        evs.push(ev(EV_SYN, SYN_REPORT, 0));
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(self.root.join("dev/input/event4"))
            .unwrap();
        f.write_all(&evs.iter().flat_map(|e| e.to_bytes()).collect::<Vec<u8>>())
            .unwrap();
    }

    /// A finger path: one report every `step_ms`.
    fn drag(&self, path: &[(i32, i32)], step_ms: u64) {
        for p in path {
            self.report(&[*p]);
            std::thread::sleep(Duration::from_millis(step_ms));
        }
    }

    /// Everything the windows received until `quiet_ms` pass without an event (max 5 s).
    fn collect(&self, quiet_ms: u64) -> Vec<(&'static str, Seen)> {
        let mut out = Vec::new();
        let mut last = Instant::now();
        let t = Instant::now();
        while last.elapsed() < Duration::from_millis(quiet_ms)
            && t.elapsed() < Duration::from_secs(5)
        {
            match self.conn.poll_for_event().unwrap() {
                Some(e) => {
                    let (w, s) = match e {
                        Event::ButtonPress(b) => (b.event, Seen::Press(b.detail)),
                        Event::ButtonRelease(b) => (b.event, Seen::Release(b.detail)),
                        Event::MotionNotify(m) => (m.event, Seen::Motion(m.event_x, m.event_y)),
                        _ => continue,
                    };
                    if let Some((name, _)) = self.wins.iter().find(|(_, id)| *id == w) {
                        out.push((*name, s));
                    }
                    last = Instant::now();
                }
                None => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        out
    }
}

const APPS: &str = r#"[
 {"id":"scroll","exec":["true"],"wm_class":"scrollapp","touch":"scroll"},
 {"id":"mouse","exec":["true"],"wm_class":"mouseapp","touch":"mouse"}
]"#;

fn presses(v: &[(&str, Seen)], win: &str, b: u8) -> usize {
    v.iter()
        .filter(|(w, s)| *w == win && *s == Seen::Press(b))
        .count()
}

#[test]
fn drag_in_a_scroll_app_is_wheel_events_and_never_a_button_1() {
    let Some(r) = Rig::new("scroll", APPS) else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    // finger moves up 300 px over ~250 ms: wheel down (button 5), about 6 clicks
    let path: Vec<(i32, i32)> = (0..16).map(|i| (200, 500 - i * 20)).collect();
    r.drag(&path, 16);
    let seen = r.collect(700);
    assert!(presses(&seen, "scrollapp", 5) >= 5, "{seen:?}");
    assert_eq!(presses(&seen, "scrollapp", 4), 0);
    assert_eq!(presses(&seen, "scrollapp", 1), 0, "no button 1: {seen:?}");
    // finger moves down: wheel up
    let path: Vec<(i32, i32)> = (0..16).map(|i| (200, 100 + i * 20)).collect();
    r.drag(&path, 16);
    let seen = r.collect(700);
    assert!(presses(&seen, "scrollapp", 4) >= 5, "{seen:?}");
    // sideways: 6/7
    let path: Vec<(i32, i32)> = (0..16).map(|i| (300 - i * 15, 300)).collect();
    r.drag(&path, 16);
    let seen = r.collect(700);
    assert!(presses(&seen, "scrollapp", 7) >= 3, "{seen:?}");
}

#[test]
fn tap_clicks_button_1_and_a_long_press_clicks_button_3() {
    let Some(r) = Rig::new("tap", APPS) else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    r.drag(&[(100, 200), (101, 200)], 30);
    let seen = r.collect(500);
    assert_eq!(presses(&seen, "scrollapp", 1), 1, "{seen:?}");
    assert!(seen.contains(&("scrollapp", Seen::Release(1))));

    // hold 650 ms in place, then lift
    let hold: Vec<(i32, i32)> = (0..22).map(|i| (150 + i % 2, 300)).collect();
    r.drag(&hold, 30);
    let seen = r.collect(500);
    assert_eq!(presses(&seen, "scrollapp", 3), 1, "{seen:?}");
    assert_eq!(presses(&seen, "scrollapp", 1), 0);
}

#[test]
fn long_press_then_drag_is_a_button_1_drag_for_selecting_text() {
    let Some(r) = Rig::new("select", APPS) else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let mut path: Vec<(i32, i32)> = (0..20).map(|_| (100, 300)).collect(); // 600 ms still
    path.extend((1..=10).map(|i| (100 + i * 20, 300)));
    r.drag(&path, 30);
    let seen = r.collect(600);
    let mine: Vec<_> = seen.iter().filter(|(w, _)| *w == "scrollapp").collect();
    let first_press = mine
        .iter()
        .position(|(_, s)| *s == Seen::Press(1))
        .expect("press 1");
    let rel = mine
        .iter()
        .position(|(_, s)| *s == Seen::Release(1))
        .expect("release 1");
    assert!(first_press < rel);
    let motions = mine[first_press..rel]
        .iter()
        .filter(|(_, s)| matches!(s, Seen::Motion(..)))
        .count();
    assert!(motions >= 3, "dragged: {mine:?}");
    assert_eq!(presses(&seen, "scrollapp", 3), 0, "not a right click");
    assert_eq!(
        presses(&seen, "scrollapp", 4) + presses(&seen, "scrollapp", 5),
        0,
        "a selection never scrolls"
    );
}

#[test]
fn mouse_apps_and_shell_windows_keep_the_mouse_drag() {
    let Some(r) = Rig::new("mouse", APPS) else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    for (win, x) in [("mouseapp", 600), ("walpad-testshell", 300)] {
        let y0 = if win == "mouseapp" { 500 } else { 1100 };
        let path: Vec<(i32, i32)> = (0..12).map(|i| (x, y0 - i * 20)).collect();
        r.drag(&path, 16);
        let seen = r.collect(500);
        let mine: Vec<_> = seen.iter().filter(|(w, _)| *w == win).collect();
        assert_eq!(presses(&seen, win, 1), 1, "{win}: {seen:?}");
        assert!(seen.contains(&(win, Seen::Release(1))), "{win}");
        assert!(
            mine.iter()
                .filter(|(_, s)| matches!(s, Seen::Motion(..)))
                .count()
                >= 5,
            "{win}: pointer follows the finger"
        );
        assert_eq!(
            presses(&seen, win, 4) + presses(&seen, win, 5),
            0,
            "{win}: no wheel"
        );
    }
}

#[test]
fn double_tap_arrives_as_two_clicks_on_one_spot() {
    let Some(r) = Rig::new("double", APPS) else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    r.drag(&[(100, 200), (100, 200)], 30);
    std::thread::sleep(Duration::from_millis(220)); // the lift is detected 150 ms after the last report
    r.drag(&[(108, 204), (108, 204)], 30);
    let seen = r.collect(500);
    assert_eq!(presses(&seen, "scrollapp", 1), 2, "{seen:?}");
    let m: Vec<_> = seen
        .iter()
        .filter(|(_, s)| matches!(s, Seen::Motion(..)))
        .collect();
    // the second click was snapped to the first one's spot: every motion is at (99, 199)
    assert!(m.iter().all(|(_, s)| *s == Seen::Motion(99, 199)), "{m:?}");
}

#[test]
fn two_fingers_apart_is_ctrl_wheel_up() {
    let Some(r) = Rig::new("pinch", APPS) else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    // two fingers moving apart (protocol A: two contacts per frame)
    for i in 0..14 {
        r.report(&[(200 - i * 8, 300), (260 + i * 8, 300)]);
        std::thread::sleep(Duration::from_millis(20));
    }
    let seen = r.collect(600);
    assert!(
        presses(&seen, "scrollapp", 4) >= 2,
        "ctrl+wheel up: {seen:?}"
    );
    assert_eq!(presses(&seen, "scrollapp", 1), 0);
}
