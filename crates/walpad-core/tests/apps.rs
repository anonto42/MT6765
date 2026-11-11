//! M6.1-M6.3 process manager against a real Xvfb: single-instance focus, running detection from
//! X windows, Close app via WM_DELETE_WINDOW with the SIGTERM fallback, crash versus clean exit
//! notices. The "apps" are tiny x11rb clients (xterm is not in the CI image) and shell one-liners.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use walpad_core::{daemon::Daemon, fake};
use walpad_hw::Root;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ConnectionExt as _, CreateWindowAux, EventMask, PropMode,
    WindowClass,
};
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;

static NEXT: AtomicU32 = AtomicU32::new(0);

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

struct Xvfb(Child, PathBuf);
impl Drop for Xvfb {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
/// Unique display per test: base from the pid, plus a counter (X tests run in parallel).
fn xvfb() -> Option<Xvfb> {
    let n = 120 + (std::process::id() % 20) * 8 + NEXT.fetch_add(1, Ordering::Relaxed) % 8;
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
        std::thread::sleep(Duration::from_millis(50));
    }
    sock.exists().then_some(Xvfb(child, sock))
}

fn atom(c: &impl Connection, name: &str) -> u32 {
    c.intern_atom(false, name.as_bytes())
        .unwrap()
        .reply()
        .unwrap()
        .atom
}

/// A client window that behaves like a simple app.
struct TestWin {
    id: u32,
    gone: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    h: Option<std::thread::JoinHandle<()>>,
}
struct WinSpec<'a> {
    class: Option<&'a str>,
    title: Option<&'a str>,
    pid: Option<u32>,
    /// true: destroy the window on WM_DELETE_WINDOW; false: advertise it but ignore it
    obey_delete: bool,
}
impl TestWin {
    fn open(sock: &Path, spec: WinSpec<'_>) -> TestWin {
        let xc = walpad_x::connect(sock).unwrap();
        let (conn, root) = (xc.conn, xc.root);
        let id = conn.generate_id().unwrap();
        conn.create_window(
            0,
            id,
            root,
            0,
            0,
            200,
            200,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new(),
        )
        .unwrap();
        if let Some(c) = spec.class {
            let v = format!("{c}\0{c}\0");
            conn.change_property8(
                PropMode::REPLACE,
                id,
                AtomEnum::WM_CLASS,
                AtomEnum::STRING,
                v.as_bytes(),
            )
            .unwrap();
        }
        if let Some(t) = spec.title {
            conn.change_property8(
                PropMode::REPLACE,
                id,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                t.as_bytes(),
            )
            .unwrap();
        }
        if let Some(p) = spec.pid {
            let a = atom(&conn, "_NET_WM_PID");
            conn.change_property32(PropMode::REPLACE, id, a, AtomEnum::CARDINAL, &[p])
                .unwrap();
        }
        let proto = atom(&conn, "WM_PROTOCOLS");
        let del = atom(&conn, "WM_DELETE_WINDOW");
        conn.change_property32(PropMode::REPLACE, id, proto, AtomEnum::ATOM, &[del])
            .unwrap();
        conn.map_window(id).unwrap();
        conn.flush().unwrap();
        let (gone, stop) = (
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        let (g2, s2, obey) = (gone.clone(), stop.clone(), spec.obey_delete);
        let h = std::thread::spawn(move || {
            while !s2.load(Ordering::Relaxed) {
                match conn.poll_for_event() {
                    Ok(Some(Event::ClientMessage(m))) if m.data.as_data32()[0] == del => {
                        if obey {
                            conn.destroy_window(id).unwrap();
                            conn.flush().unwrap();
                            g2.store(true, Ordering::Relaxed);
                        }
                    }
                    Ok(Some(_)) => {}
                    _ => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        });
        TestWin {
            id,
            gone,
            stop,
            h: Some(h),
        }
    }
}
impl Drop for TestWin {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.h.take() {
            let _ = h.join();
        }
    }
}

/// Plays the window manager: records _NET_ACTIVE_WINDOW requests sent to the root.
struct Wm {
    active: Arc<Mutex<Vec<u32>>>,
    stop: Arc<AtomicBool>,
    h: Option<std::thread::JoinHandle<()>>,
}
impl Wm {
    fn start(sock: &Path) -> Wm {
        let xc = walpad_x::connect(sock).unwrap();
        let (conn, root) = (xc.conn, xc.root);
        conn.change_window_attributes(
            root,
            &ChangeWindowAttributesAux::new()
                .event_mask(EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY),
        )
        .unwrap()
        .check()
        .unwrap();
        let na = atom(&conn, "_NET_ACTIVE_WINDOW");
        let active = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (a2, s2) = (active.clone(), stop.clone());
        let h = std::thread::spawn(move || {
            while !s2.load(Ordering::Relaxed) {
                match conn.poll_for_event() {
                    Ok(Some(Event::ClientMessage(m))) if m.type_ == na => {
                        a2.lock().unwrap().push(m.window)
                    }
                    Ok(Some(_)) => {}
                    _ => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        });
        Wm {
            active,
            stop,
            h: Some(h),
        }
    }
    fn activated(&self) -> Vec<u32> {
        self.active.lock().unwrap().clone()
    }
}
impl Drop for Wm {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.h.take() {
            let _ = h.join();
        }
    }
}

struct Running {
    stop: Arc<AtomicBool>,
    h: Option<std::thread::JoinHandle<()>>,
    root: PathBuf,
}
impl Running {
    fn start(name: &str, apps: &str, x: Option<&Path>) -> Running {
        let root =
            std::env::temp_dir().join(format!("walpad-apps-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("usr/bin")).unwrap();
        std::fs::create_dir_all(root.join("etc/walpad")).unwrap();
        fake::populate(&root).unwrap();
        for b in ["sleep", "sh"] {
            std::fs::write(root.join("usr/bin").join(b), "").unwrap();
        }
        std::fs::write(root.join("etc/walpad/apps.json"), apps).unwrap();
        if let Some(x) = x {
            std::fs::create_dir_all(root.join("tmp/.X11-unix")).unwrap();
            std::os::unix::fs::symlink(x, root.join("tmp/.X11-unix/X0")).unwrap();
        }
        let mut d = Daemon::new(Root::new(&root)).unwrap();
        d.set_close_grace(Duration::from_millis(400));
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        Running {
            stop,
            h: Some(std::thread::spawn(move || d.run(&s2))),
            root,
        }
    }
    fn sock(&self) -> PathBuf {
        self.root.join("run/walpad/control.sock")
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.h.take() {
            let _ = h.join();
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Client {
    w: UnixStream,
    r: BufReader<UnixStream>,
    id: u64,
    /// events seen while waiting for a reply
    events: Vec<Value>,
}
impl Client {
    fn connect(run: &Running, topics: &[&str]) -> Client {
        let s = UnixStream::connect(run.sock()).unwrap();
        s.set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let mut c = Client {
            r: BufReader::new(s.try_clone().unwrap()),
            w: s,
            id: 0,
            events: vec![],
        };
        assert_eq!(c.call("hello", json!({"client":"t","proto":1}))["ok"], true);
        assert_eq!(c.call("subscribe", json!({"topics": topics}))["ok"], true);
        c
    }
    fn read_line(&mut self) -> Option<Value> {
        let mut l = String::new();
        match self.r.read_line(&mut l) {
            Ok(n) if n > 0 => serde_json::from_str(&l).ok(),
            _ => None,
        }
    }
    fn call(&mut self, cmd: &str, args: Value) -> Value {
        self.id += 1;
        let id = self.id;
        writeln!(self.w, "{}", json!({"v":1,"id":id,"cmd":cmd,"args":args})).unwrap();
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(8) {
            if let Some(v) = self.read_line() {
                if v["id"] == id {
                    return v;
                }
                if v.get("event").is_some() {
                    self.events.push(v);
                }
            }
        }
        panic!("no reply to {cmd}");
    }
    /// Wait until an event satisfying `pred` has been seen (earlier ones are kept in `events`).
    fn wait_event(&mut self, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
        let t = Instant::now();
        let mut scanned = 0;
        while t.elapsed() < Duration::from_secs(8) {
            while scanned < self.events.len() {
                scanned += 1;
                if pred(&self.events[scanned - 1]) {
                    return self.events[scanned - 1].clone();
                }
            }
            if let Some(v) = self.read_line() {
                if v.get("event").is_some() {
                    self.events.push(v);
                }
            }
        }
        panic!("timed out waiting for event: {what}");
    }
    fn app_running(&mut self, app: &str) -> Option<bool> {
        let st = self.call("get_status", Value::Null);
        st["data"]["apps"]
            .as_array()?
            .iter()
            .find(|a| a["app"] == app)
            .map(|a| a["running"] == true)
    }
}

fn kind(app: &'static str, k: &'static str) -> impl Fn(&Value) -> bool {
    move |e| e["event"] == "apps" && e["data"]["kind"] == k && e["data"]["app"] == app
}

const SLEEPER: &str = r#"[{"id":"sleeper","name":"Sleeper","exec":["sleep","30"],"single_instance":true,"wm_class":"sleeper"}]"#;

fn sleep_child() -> Child {
    Command::new("sleep").arg("30").spawn().unwrap()
}

#[test]
fn single_instance_launch_focuses_the_existing_window_and_detects_running() {
    let Some(x) = xvfb() else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let wm = Wm::start(&x.1);
    let run = Running::start("single", SLEEPER, Some(&x.1));
    let mut c = Client::connect(&run, &["apps"]);
    // an app started by hand (not by the core), listed in _NET_CLIENT_LIST like a real WM does
    let win = TestWin::open(
        &x.1,
        WinSpec {
            class: Some("Sleeper"),
            title: None,
            pid: None,
            obey_delete: true,
        },
    );
    {
        let xc = walpad_x::connect(&x.1).unwrap();
        let a = atom(&xc.conn, "_NET_CLIENT_LIST");
        xc.conn
            .change_property32(PropMode::REPLACE, xc.root, a, AtomEnum::WINDOW, &[win.id])
            .unwrap();
        xc.conn.flush().unwrap();
        xc.conn.get_input_focus().unwrap().reply().unwrap();
    }

    // running is detected from the window, with no child of ours
    wait_for("running from the X window", || {
        c.app_running("sleeper") == Some(true)
    });
    c.wait_event("external started", |e| {
        kind("sleeper", "started")(e) && e["data"]["external"] == true
    });

    // launching a single_instance app whose window exists focuses it and spawns nothing
    let r = c.call("launch", json!({"app":"sleeper"}));
    assert_eq!(r["data"]["result"], "focused", "{r}");
    wait_for("_NET_ACTIVE_WINDOW for the app window", || {
        wm.activated().contains(&win.id)
    });
    assert!(
        !c.events
            .iter()
            .any(|e| kind("sleeper", "started")(e) && e["data"].get("pid").is_some()),
        "no second copy may be spawned"
    );
    let r = c.call("launch", json!({"app":"sleeper"}));
    assert_eq!(r["data"]["result"], "focused");

    // when the window goes away the app is no longer running
    drop(win);
    {
        let xc = walpad_x::connect(&x.1).unwrap();
        let a = atom(&xc.conn, "_NET_CLIENT_LIST");
        xc.conn.delete_property(xc.root, a).unwrap();
        xc.conn.flush().unwrap();
        xc.conn.get_input_focus().unwrap().reply().unwrap();
    }
    wait_for("not running after the window is gone", || {
        c.app_running("sleeper") == Some(false)
    });
    c.wait_event("external exit", |e| {
        kind("sleeper", "exited")(e) && e["data"]["external"] == true
    });
}

#[test]
fn without_a_window_launch_spawns_once_then_focuses_that_window() {
    let Some(x) = xvfb() else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let run = Running::start("spawn-once", SLEEPER, Some(&x.1));
    let mut c = Client::connect(&run, &["apps"]);
    let r = c.call("launch", json!({"app":"sleeper"}));
    assert_eq!(r["data"]["result"], "launched", "{r}");
    c.wait_event("started with a pid", |e| {
        kind("sleeper", "started")(e) && e["data"]["pid"].is_u64()
    });
    // no window yet: the second launch must not start another copy
    let r = c.call("launch", json!({"app":"sleeper"}));
    assert_eq!(r["data"]["result"], "already_running", "{r}");
    // the child shows a window: now the launch focuses it
    let wm = Wm::start(&x.1);
    let win = TestWin::open(
        &x.1,
        WinSpec {
            class: Some("sleeper"),
            title: None,
            pid: None,
            obey_delete: true,
        },
    );
    let r = c.call("launch", json!({"app":"sleeper"}));
    assert_eq!(r["data"]["result"], "focused", "{r}");
    wait_for("focus request", || wm.activated().contains(&win.id));
}

#[test]
fn close_app_sends_wm_delete_window_to_an_app_we_did_not_start() {
    let Some(x) = xvfb() else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    // the "process" behind the window must survive: the window closes politely, no signal
    let mut proc_ = sleep_child();
    let win = TestWin::open(
        &x.1,
        WinSpec {
            class: Some("sleeper"),
            title: None,
            pid: Some(proc_.id()),
            obey_delete: true,
        },
    );
    let run = Running::start("close-delete", SLEEPER, Some(&x.1));
    let mut c = Client::connect(&run, &["apps"]);
    wait_for("running from the X window", || {
        c.app_running("sleeper") == Some(true)
    });
    let r = c.call("close_app", json!({"app":"sleeper"}));
    assert_eq!(r["data"]["closed"], true, "{r}");
    assert_eq!(r["data"]["how"], "window_close");
    wait_for("window destroyed by WM_DELETE_WINDOW", || {
        win.gone.load(Ordering::Relaxed)
    });
    wait_for("not running any more", || {
        c.app_running("sleeper") == Some(false)
    });
    c.wait_event("external exit event", kind("sleeper", "exited"));
    std::thread::sleep(Duration::from_millis(800)); // past the grace period
    assert!(
        proc_.try_wait().unwrap().is_none(),
        "a window that closed itself must not get SIGTERM"
    );
    let _ = proc_.kill();
    let _ = proc_.wait();
    // nothing open any more: reported honestly
    let r = c.call("close_app", json!({"app":"sleeper"}));
    assert_eq!(r["data"]["closed"], false, "{r}");
}

#[test]
fn close_app_falls_back_to_sigterm_when_the_window_ignores_delete() {
    let Some(x) = xvfb() else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let mut proc_ = sleep_child();
    let _win = TestWin::open(
        &x.1,
        WinSpec {
            class: Some("sleeper"),
            title: None,
            pid: Some(proc_.id()),
            obey_delete: false,
        },
    );
    let run = Running::start("close-term", SLEEPER, Some(&x.1));
    let mut c = Client::connect(&run, &[]);
    wait_for("running from the X window", || {
        c.app_running("sleeper") == Some(true)
    });
    let t = Instant::now();
    assert_eq!(
        c.call("close_app", json!({"app":"sleeper"}))["data"]["closed"],
        true
    );
    assert!(
        proc_.try_wait().unwrap().is_none(),
        "not killed before the grace period"
    );
    wait_for("SIGTERM of _NET_WM_PID", || {
        proc_.try_wait().unwrap().is_some()
    });
    assert!(
        t.elapsed() >= Duration::from_millis(350),
        "waited for the grace period"
    );
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(
        proc_.wait().unwrap().signal(),
        Some(15),
        "SIGTERM, not SIGKILL"
    );
}

#[test]
fn crash_publishes_a_notice_and_raises_home_but_clean_exit_and_close_do_not() {
    let Some(x) = xvfb() else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let wm = Wm::start(&x.1);
    let home = TestWin::open(
        &x.1,
        WinSpec {
            class: Some("walpad-shell"),
            title: Some("WalPad Home"),
            pid: None,
            obey_delete: false,
        },
    );
    let apps = r#"[
        {"id":"fine","name":"Fine","exec":["sh","-c","exit 0"]},
        {"id":"crashy","name":"Crashy App","exec":["sh","-c","exit 3"]},
        {"id":"killed","name":"Killed","exec":["sh","-c","kill -KILL $$"]},
        {"id":"sleeper","name":"Sleeper","exec":["sleep","30"]}
    ]"#;
    let run = Running::start("crash", apps, Some(&x.1));
    let mut c = Client::connect(&run, &["apps", "notice"]);

    assert_eq!(c.call("launch", json!({"app":"fine"}))["ok"], true);
    c.wait_event("fine exited", kind("fine", "exited"));

    // an app we close ourselves exits by SIGTERM: that is not a crash
    assert_eq!(c.call("launch", json!({"app":"sleeper"}))["ok"], true);
    let r = c.call("close_app", json!({"app":"sleeper"}));
    assert_eq!(
        (r["data"]["closed"].clone(), r["data"]["how"].clone()),
        (json!(true), json!("terminate"))
    );
    let e = c.wait_event("sleeper exited", kind("sleeper", "exited"));
    assert_eq!(e["data"]["closed"], true);
    assert!(wm.activated().is_empty(), "no raise without a crash");

    // non-zero exit: crashed event + notice + shell raised
    assert_eq!(c.call("launch", json!({"app":"crashy"}))["ok"], true);
    let e = c.wait_event("crashy crashed", kind("crashy", "crashed"));
    assert_eq!(e["data"]["code"], 3);
    let n = c.wait_event("crash notice", |e| e["event"] == "notice");
    assert_eq!(
        n["data"],
        json!({"kind":"app_crashed","app":"crashy","name":"Crashy App"})
    );
    wait_for("shell window raised", || wm.activated().contains(&home.id));

    // death by signal counts too
    assert_eq!(c.call("launch", json!({"app":"killed"}))["ok"], true);
    let e = c.wait_event("killed crashed", kind("killed", "crashed"));
    assert_eq!(e["data"]["signal"], 9);
    c.wait_event("second notice", |e| {
        e["event"] == "notice" && e["data"]["app"] == "killed"
    });

    // exactly two notices in total: the clean exit and our own close raised none
    let notices: Vec<_> = c.events.iter().filter(|e| e["event"] == "notice").collect();
    assert_eq!(notices.len(), 2, "{notices:?}");
    assert_eq!(c.app_running("crashy"), Some(false));
}

#[test]
fn status_never_lists_an_app_whose_executable_is_missing() {
    let apps = r#"[
        {"id":"sleeper","name":"S","exec":["sleep","30"]},
        {"id":"ghost","name":"G","exec":["definitely-not-installed"],"wm_class":"ghost"}
    ]"#;
    let run = Running::start("missing", apps, None);
    let mut c = Client::connect(&run, &[]);
    let st = c.call("get_status", Value::Null);
    assert_eq!(
        st["data"]["apps"],
        json!([{"app":"sleeper","running":false}])
    );
    assert_eq!(
        c.call("launch", json!({"app":"ghost"}))["error"]["code"],
        "refused"
    );
    assert_eq!(
        c.call("close_app", json!({"app":"ghost"}))["data"]["closed"],
        false
    );
    // the executable vanishes while the core runs: the app disappears from the list
    std::fs::remove_file(run.root.join("usr/bin/sleep")).unwrap();
    let st = c.call("get_status", Value::Null);
    assert_eq!(st["data"]["apps"], json!([]));
}

// ---- change multitasking-windows: instances, window list, split / free layout

const MULTI: &str = r#"[{"id":"term","name":"Term","exec":["sleep","30"],"max_instances":2},{"id":"solo","name":"Solo","exec":["sleep","30"],"single_instance":true}]"#;

fn geometry(sock: &Path, win: u32) -> (i32, i32, i32, i32) {
    let xc = walpad_x::connect(sock).unwrap();
    let g = xc.conn.get_geometry(win).unwrap().reply().unwrap();
    let t = xc
        .conn
        .translate_coordinates(win, xc.root, 0, 0)
        .unwrap()
        .reply()
        .unwrap();
    (
        t.dst_x as i32,
        t.dst_y as i32,
        g.width as i32,
        g.height as i32,
    )
}

fn app_win(sock: &Path, class: &str) -> TestWin {
    TestWin::open(
        sock,
        WinSpec {
            class: Some(class),
            title: Some(class),
            pid: None,
            obey_delete: true,
        },
    )
}

#[test]
fn new_instance_starts_more_copies_up_to_the_limit_and_only_for_apps_that_allow_it() {
    let run = Running::start("multi", MULTI, None);
    let mut c = Client::connect(&run, &["apps"]);
    let r = c.call("new_instance", json!({"app":"term"}));
    assert_eq!(r["data"]["instance"], 1, "{r}");
    let r = c.call("new_instance", json!({"app":"term"}));
    assert_eq!(r["data"]["instance"], 2, "{r}");
    let r = c.call("new_instance", json!({"app":"term"}));
    assert_eq!(r["error"]["code"], "refused", "{r}");
    let r = c.call("new_instance", json!({"app":"solo"}));
    assert_eq!(r["error"]["code"], "refused", "{r}");
    let r = c.call("new_instance", json!({"app":"nope"}));
    assert_eq!(r["error"]["code"], "invalid_args", "{r}");
    assert_eq!(c.app_running("term"), Some(true));
    // Close app closes every copy and the app is gone afterwards
    let r = c.call("close_app", json!({"app":"term"}));
    assert_eq!(r["data"]["closed"], true, "{r}");
    c.wait_event("exited", kind("term", "exited"));
    wait_for("not running", || c.app_running("term") == Some(false));
}

#[test]
fn windows_lists_app_windows_and_close_window_closes_just_one() {
    let Some(x) = xvfb() else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let run = Running::start("winlist", MULTI, Some(&x.1));
    let mut c = Client::connect(&run, &[]);
    let a = app_win(&x.1, "st");
    let b = app_win(&x.1, "st");
    let r = c.call("windows", Value::Null);
    let list = r["data"]["windows"].as_array().unwrap().clone();
    let ids: Vec<u64> = list.iter().map(|w| w["window"].as_u64().unwrap()).collect();
    assert!(
        ids.contains(&(a.id as u64)) && ids.contains(&(b.id as u64)),
        "{r}"
    );
    assert_eq!(r["data"]["layout"]["mode"], "single");
    // unknown windows are refused
    let r = c.call("close_window", json!({"window": 12345}));
    assert_eq!(r["error"]["code"], "invalid_args", "{r}");
    let r = c.call("close_window", json!({"window": a.id}));
    assert_eq!(r["ok"], true, "{r}");
    wait_for("window a gone", || a.gone.load(Ordering::Relaxed));
    assert!(!b.gone.load(Ordering::Relaxed));
}

#[test]
fn split_tiles_two_windows_stacked_then_side_by_side_after_rotation() {
    let Some(x) = xvfb() else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let run = Running::start("split", MULTI, Some(&x.1));
    let mut c = Client::connect(&run, &["apps"]);
    let a = app_win(&x.1, "alpha");
    let b = app_win(&x.1, "beta");
    // single mode: windows are left alone
    assert_eq!(c.call("layout", Value::Null)["data"]["mode"], "single");
    // placing by hand is a free-mode feature
    let r = c.call(
        "window_rect",
        json!({"window": a.id, "x": 0, "y": 0, "w": 400, "h": 300}),
    );
    assert_eq!(r["error"]["code"], "refused", "{r}");
    let r = c.call("layout", json!({"mode":"split"}));
    assert_eq!(r["data"]["mode"], "split", "{r}");
    c.wait_event("layout event", |e| e["data"]["kind"] == "layout");
    // portrait (800 wide): stacked, the two together fill 800x1280
    wait_for("stacked tiles", || {
        let (ga, gb) = (geometry(&x.1, a.id), geometry(&x.1, b.id));
        ga.2 == 800 && gb.2 == 800 && ga.3 + gb.3 + 4 == 1280 && ga.1 != gb.1
    });
    // rotate to landscape: side by side in 1280x800
    let r = c.call("rotate", json!({"mode":"90"}));
    assert_eq!(r["ok"], true, "{r}");
    wait_for("side by side tiles", || {
        let (ga, gb) = (geometry(&x.1, a.id), geometry(&x.1, b.id));
        ga.3 == 800 && gb.3 == 800 && ga.2 + gb.2 + 4 == 1280 && ga.0 != gb.0
    });
    // ratio 1/3 and swap
    let r = c.call("layout", json!({"ratio":33}));
    assert_eq!(r["data"]["ratio"], 33);
    wait_for("1/3 tile", || {
        let (ga, gb) = (geometry(&x.1, a.id), geometry(&x.1, b.id));
        ga.2.min(gb.2) == (1280 - 4) * 33 / 100
    });
    let before = (geometry(&x.1, a.id).0, geometry(&x.1, b.id).0);
    c.call("layout", json!({"swap":true}));
    wait_for("swapped", || {
        let now = (geometry(&x.1, a.id).0, geometry(&x.1, b.id).0);
        (now.0 > now.1) != (before.0 > before.1)
    });
    // a bad ratio is rejected by the parser
    let r = c.call("layout", json!({"ratio":40}));
    assert_eq!(r["error"]["code"], "invalid_args", "{r}");
}

#[test]
fn free_mode_clamps_moves_and_resizes_and_snaps() {
    let Some(x) = xvfb() else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let run = Running::start("free", MULTI, Some(&x.1));
    let mut c = Client::connect(&run, &[]);
    let a = app_win(&x.1, "alpha");
    c.call("layout", json!({"mode":"free"}));
    // the work area is the portrait panel 800x1280 (rotation strut)
    wait_for("floating window inside the panel", || {
        let g = geometry(&x.1, a.id);
        g.2 < 800 && g.2 >= 320 && g.0 + g.2 <= 800 && g.1 + g.3 <= 1280
    });
    // far outside and too small: clamped
    let r = c.call(
        "window_rect",
        json!({"window": a.id, "x": 700, "y": 1200, "w": 100, "h": 100}),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        (r["data"]["w"].clone(), r["data"]["h"].clone()),
        (json!(320), json!(240))
    );
    wait_for("clamped window", || {
        geometry(&x.1, a.id) == (480, 1040, 320, 240)
    });
    let r = c.call("snap", json!({"window": a.id, "zone":"left"}));
    assert_eq!(r["ok"], true, "{r}");
    wait_for("left half", || geometry(&x.1, a.id) == (0, 0, 398, 1280));
    let r = c.call("snap", json!({"window": a.id, "zone":"full"}));
    assert_eq!(r["data"]["w"], 800);
    wait_for("full", || geometry(&x.1, a.id) == (0, 0, 800, 1280));
    // a window that is not ours is refused
    let r = c.call("snap", json!({"window": 77, "zone":"full"}));
    assert_eq!(r["error"]["code"], "invalid_args", "{r}");
}
