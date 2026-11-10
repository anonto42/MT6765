//! Live-mode integration: recorded events in a fake root drive the real executors.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use walpad_core::xbridge::XBridge;
use walpad_core::{daemon::Daemon, fake};
use walpad_hw::evdev::*;
use walpad_hw::Root;

fn ev(kind: u16, code: u16, value: i32) -> InputEvent {
    InputEvent {
        sec: 1,
        usec: 0,
        kind,
        code,
        value,
    }
}
fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("walpad-live-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}
fn put_device(root: &Path, n: u32, name: &str, events: &[InputEvent]) {
    let dir = root.join(format!("sys/class/input/input{n}"));
    std::fs::create_dir_all(dir.join(format!("event{n}"))).unwrap();
    std::fs::write(dir.join("name"), format!("{name}\n")).unwrap();
    std::fs::write(
        dir.join(format!("event{n}/dev")),
        format!("13:{}\n", 64 + n),
    )
    .unwrap();
    std::fs::create_dir_all(root.join("dev/input")).unwrap();
    let bytes: Vec<u8> = events.iter().flat_map(|e| e.to_bytes()).collect();
    std::fs::write(root.join(format!("dev/input/event{n}")), bytes).unwrap();
}
fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel))
        .unwrap_or_default()
        .trim()
        .to_string()
}
fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(5) {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    panic!("timed out waiting for {what}");
}

struct Running {
    stop: Arc<AtomicBool>,
    h: Option<std::thread::JoinHandle<()>>,
}
impl Running {
    fn start(mut d: Daemon) -> Running {
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        Running {
            stop,
            h: Some(std::thread::spawn(move || d.run(&s2))),
        }
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.h.take() {
            let _ = h.join();
        }
    }
}

fn rpc(root: &Path, cmd: &str, args: Value) -> Value {
    let s = UnixStream::connect(root.join("run/walpad/control.sock")).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut w = s.try_clone().unwrap();
    let mut r = BufReader::new(s);
    let mut call = |id: u64, cmd: &str, args: Value| -> Value {
        writeln!(w, "{}", json!({"v":1,"id":id,"cmd":cmd,"args":args})).unwrap();
        let mut l = String::new();
        r.read_line(&mut l).unwrap();
        serde_json::from_str(&l).unwrap()
    };
    call(1, "hello", json!({"client":"t","proto":1}));
    call(2, cmd, args)
}

#[test]
fn power_key_short_press_turns_screen_off_via_live_device() {
    let root = tmp("power");
    fake::populate(&root).unwrap();
    put_device(
        &root,
        2,
        "mtk-pmic-keys",
        &[ev(EV_KEY, KEY_POWER, 1), ev(EV_KEY, KEY_POWER, 0)],
    );
    let mut d = Daemon::new(Root::new(&root)).unwrap();
    d.enable_live(true, false);
    let _r = Running::start(d);
    wait_for("backlight 0 after power press", || {
        read(&root, "sys/class/leds/lcd-backlight/brightness") == "0"
    });
    assert_eq!(
        rpc(&root, "get_status", Value::Null)["data"]["screen"],
        "off"
    );
}

#[test]
fn cover_closed_turns_screen_off_and_gsensor_rotates() {
    let root = tmp("cover");
    fake::populate(&root).unwrap();
    put_device(&root, 0, "HALL_DEV", &[ev(EV_SW, 0, 1)]);
    let mut d = Daemon::new(Root::new(&root)).unwrap();
    d.enable_live(true, false);
    let _r = Running::start(d);
    wait_for("cover -> backlight 0", || {
        read(&root, "sys/class/leds/lcd-backlight/brightness") == "0"
    });

    // gsensor fixture says "tilted to x": after 3 samples the orientation file becomes 90
    let root2 = tmp("rotate");
    fake::populate(&root2).unwrap();
    std::fs::write(root2.join("dev/gsensor"), "00000fa0 00000064 00000000").unwrap();
    let mut d = Daemon::new(Root::new(&root2)).unwrap();
    d.enable_live(true, false);
    let _r2 = Running::start(d);
    wait_for("orientation 90", || {
        read(&root2, "run/walpad/orientation") == "90"
    });
}

#[test]
fn flags_decide_which_subsystem_acts() {
    // device=false: a power press must NOT touch the backlight
    let root = tmp("flags");
    fake::populate(&root).unwrap();
    put_device(
        &root,
        2,
        "mtk-pmic-keys",
        &[ev(EV_KEY, KEY_POWER, 1), ev(EV_KEY, KEY_POWER, 0)],
    );
    let mut d = Daemon::new(Root::new(&root)).unwrap();
    d.enable_live(false, true);
    let _r = Running::start(d);
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(
        read(&root, "sys/class/leds/lcd-backlight/brightness"),
        "120"
    );
}

struct Xvfb(Child);
impl Drop for Xvfb {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn touch_drives_the_x_pointer_in_every_orientation() {
    let n = 30 + std::process::id() % 30;
    let Ok(child) = Command::new("Xvfb")
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
    else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let _x = Xvfb(child);
    let sock = PathBuf::from(format!("/tmp/.X11-unix/X{n}"));
    wait_for("Xvfb socket", || sock.exists());
    let probe = XBridge::connect(&sock).unwrap();

    // panel point (100,200) calibrates to (99,199); then the contract rotation table applies
    for (o, want) in [
        (0, (99i16, 199i16)),
        (180, (700, 1080)),
        (90, (199, 700)),
        (270, (1080, 99)),
    ] {
        let root = tmp(&format!("touch{o}"));
        fake::populate(&root).unwrap();
        std::fs::create_dir_all(root.join("tmp/.X11-unix")).unwrap();
        std::os::unix::fs::symlink(&sock, root.join("tmp/.X11-unix/X0")).unwrap();
        std::fs::write(
            root.join("etc/walpad/devd.json"),
            r#"{"auto_rotate":false}"#,
        )
        .unwrap();
        put_device(&root, 4, "mtk-tpd", &[]);
        let mut d = Daemon::new(Root::new(&root)).unwrap();
        d.enable_live(true, true);
        let _r = Running::start(d);

        // set the orientation first, then let the finger touch (events appended to the device file)
        assert_eq!(
            rpc(&root, "rotate", json!({"mode": o.to_string()}))["ok"],
            true
        );
        wait_for("orientation file", || {
            read(&root, "run/walpad/orientation") == o.to_string()
        });
        let evs = [
            ev(EV_ABS, ABS_MT_POSITION_X, 100),
            ev(EV_ABS, ABS_MT_POSITION_Y, 200),
            ev(EV_SYN, SYN_REPORT, 0),
        ];
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(root.join("dev/input/event4"))
            .unwrap();
        f.write_all(&evs.iter().flat_map(|e| e.to_bytes()).collect::<Vec<u8>>())
            .unwrap();
        wait_for(&format!("pointer at {want:?} for orientation {o}"), || {
            probe.pointer_position().map(|p| p == want).unwrap_or(false)
        });
    }
}

/// A shell-like client: hello + subscribe(input); `next_input` returns the next `input` event.
struct ShellClient {
    w: UnixStream,
    r: BufReader<UnixStream>,
}
impl ShellClient {
    fn connect(root: &Path) -> ShellClient {
        let s = UnixStream::connect(root.join("run/walpad/control.sock")).unwrap();
        s.set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let mut c = ShellClient {
            w: s.try_clone().unwrap(),
            r: BufReader::new(s),
        };
        c.send(1, "hello", json!({"client": "t", "proto": 1}));
        c.send(2, "subscribe", json!({"topics": ["input"]}));
        c
    }
    fn send(&mut self, id: u64, cmd: &str, args: Value) {
        writeln!(self.w, "{}", json!({"v":1,"id":id,"cmd":cmd,"args":args})).unwrap();
    }
    /// Next `input` event payload within `secs`, skipping responses.
    fn next_input(&mut self, secs: u64) -> Option<Value> {
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(secs) {
            let mut l = String::new();
            if self.r.read_line(&mut l).is_ok() && !l.is_empty() {
                let v: Value = serde_json::from_str(&l).unwrap();
                if v["event"] == "input" {
                    return Some(v["data"].clone());
                }
            }
        }
        None
    }
}

fn append_events(root: &Path, n: u32, evs: &[InputEvent]) {
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(root.join(format!("dev/input/event{n}")))
        .unwrap();
    f.write_all(&evs.iter().flat_map(|e| e.to_bytes()).collect::<Vec<u8>>())
        .unwrap();
}

#[test]
fn long_power_opens_the_shell_menu_and_the_keys_are_routed_to_the_shell() {
    let root = tmp("shellmenu");
    fake::populate(&root).unwrap();
    put_device(&root, 1, "mtk-kpd", &[]);
    put_device(&root, 2, "mtk-pmic-keys", &[]);
    let mut d = Daemon::new(Root::new(&root)).unwrap();
    d.enable_live(true, true);
    let _r = Running::start(d);
    let mut shell = ShellClient::connect(&root);
    std::thread::sleep(Duration::from_millis(300));

    // hold power for 1.3 s (the core's clock is real time): the shell is told to open the menu
    append_events(&root, 2, &[ev(EV_KEY, KEY_POWER, 1)]);
    std::thread::sleep(Duration::from_millis(1300));
    append_events(&root, 2, &[ev(EV_KEY, KEY_POWER, 0)]);
    assert_eq!(
        shell.next_input(3),
        Some(json!({"kind": "power_menu_open"}))
    );

    // while the menu is open volume down / up and a short power press reach the shell
    append_events(
        &root,
        1,
        &[ev(EV_KEY, KEY_VOLUMEDOWN, 1), ev(EV_KEY, KEY_VOLUMEDOWN, 0)],
    );
    assert_eq!(shell.next_input(3), Some(json!({"kind": "volume_down"})));
    append_events(
        &root,
        1,
        &[ev(EV_KEY, KEY_VOLUMEUP, 1), ev(EV_KEY, KEY_VOLUMEUP, 0)],
    );
    assert_eq!(shell.next_input(3), Some(json!({"kind": "volume_up"})));
    append_events(
        &root,
        2,
        &[ev(EV_KEY, KEY_POWER, 1), ev(EV_KEY, KEY_POWER, 0)],
    );
    assert_eq!(shell.next_input(3), Some(json!({"kind": "power_short"})));
    // a short power press while the menu is open must not toggle the screen
    assert_eq!(
        read(&root, "sys/class/leds/lcd-backlight/brightness"),
        "120"
    );

    // the shell closes its menu: volume keys are ordinary again (no input event)
    shell.send(3, "power_menu", json!({"open": false}));
    std::thread::sleep(Duration::from_millis(300));
    append_events(
        &root,
        1,
        &[ev(EV_KEY, KEY_VOLUMEUP, 1), ev(EV_KEY, KEY_VOLUMEUP, 0)],
    );
    assert_eq!(shell.next_input(1), None);
}

#[test]
fn without_a_subscribed_shell_the_old_menu_path_is_used() {
    // no shell client: the long press must not publish anything and must not panic; the Tk menu
    // is spawned (walpad-powermenu.py does not exist in the test environment: logged and ignored)
    let root = tmp("nomenu");
    fake::populate(&root).unwrap();
    put_device(&root, 2, "mtk-pmic-keys", &[]);
    let mut d = Daemon::new(Root::new(&root)).unwrap();
    d.enable_live(true, true);
    let _r = Running::start(d);
    std::thread::sleep(Duration::from_millis(300));
    append_events(&root, 2, &[ev(EV_KEY, KEY_POWER, 1)]);
    std::thread::sleep(Duration::from_millis(1300));
    append_events(&root, 2, &[ev(EV_KEY, KEY_POWER, 0)]);
    std::thread::sleep(Duration::from_millis(300));
    // a late subscriber sees no replayed event and the screen is still on
    let mut late = ShellClient::connect(&root);
    assert_eq!(late.next_input(1), None);
    assert_eq!(
        read(&root, "sys/class/leds/lcd-backlight/brightness"),
        "120"
    );
}

/// Volume keys mapped to `volume:+5` / `volume:-5` change the level through walpad-volume and tell
/// the shell with an `audio` event (source "key"; owner request 2026-11-10).
#[test]
fn volume_key_actions_change_the_level_and_publish_an_audio_event() {
    use std::os::unix::fs::PermissionsExt;
    let root = tmp("volkeys");
    fake::populate(&root).unwrap();
    put_device(&root, 1, "mtk-kpd", &[]);
    put_device(&root, 2, "mtk-pmic-keys", &[]);
    std::fs::write(
        root.join("etc/walpad/keys.json"),
        r#"{"tap_window_ms":120,"long_ms":600,"vol_up":{"single":"volume:+5"},"vol_down":{"single":"volume:-5"}}"#,
    )
    .unwrap();
    let s = root.join("usr/local/sbin/walpad-volume");
    std::fs::create_dir_all(s.parent().unwrap()).unwrap();
    std::fs::write(
        &s,
        "#!/bin/sh\necho \"$*\" >> \"$WALPAD_VOLUME_ROOT/vol-calls\"\nprintf 'volume=%s\\nmuted=%s\\navailable=1\\nbackend=pulse\\n' \"${2:-1}\" \"${3:-0}\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&s, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut d = Daemon::new(Root::new(&root)).unwrap();
    d.enable_live(true, true);
    let _r = Running::start(d);
    let mut c = ShellClient::connect(&root);
    c.send(3, "subscribe", json!({"topics": ["input", "status"]}));
    std::thread::sleep(Duration::from_millis(300));
    append_events(
        &root,
        1,
        &[ev(EV_KEY, KEY_VOLUMEUP, 1), ev(EV_KEY, KEY_VOLUMEUP, 0)],
    );
    let t = Instant::now();
    let mut got = None;
    while got.is_none() && t.elapsed() < Duration::from_secs(5) {
        let mut l = String::new();
        if c.r.read_line(&mut l).is_ok() && !l.is_empty() {
            let v: Value = serde_json::from_str(&l).unwrap();
            if v["event"] == "audio" {
                got = Some(v["data"].clone());
            }
        }
    }
    let a = got.expect("no audio event for the volume key");
    assert_eq!(
        (&a["volume"], &a["source"]),
        (&json!(65), &json!("key")),
        "{a}"
    );
    wait_for("walpad-volume apply 65", || {
        read(&root, "vol-calls").contains("apply 65 0")
    });
    append_events(
        &root,
        1,
        &[ev(EV_KEY, KEY_VOLUMEDOWN, 1), ev(EV_KEY, KEY_VOLUMEDOWN, 0)],
    );
    wait_for("walpad-volume apply 60", || {
        read(&root, "vol-calls").contains("apply 60 0")
    });
}
