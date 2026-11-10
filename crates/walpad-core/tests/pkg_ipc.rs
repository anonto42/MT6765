//! App store through the daemon's IPC socket (change app-catalogue-manager). walpad-pkg is faked
//! inside the test root (protocol level: progress, failure, hang), and once the real helper script
//! runs against a fake apk (end to end). apk is never run for real.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use walpad_core::{daemon::Daemon, fake as fakeroot};
use walpad_hw::Root;

const CATALOGUE: &str = r#"{"version":1,"reserve_kb":2097152,"entries":[
{"id":"browser","name":"Browser","category":"Built in","packages":["firefox-esr"],"size_kb":224000,"exec":["sleep","30"],"wm_class":"","desktop":"firefox-esr.desktop","roles":["browser"],"system":true},
{"id":"mousepad","name":"Mousepad","category":"Documents","packages":["mousepad"],"size_kb":700,"exec":["sleep","30"],"wm_class":"","desktop":"org.xfce.mousepad.desktop","icon":"apps","summary":"Text editor"},
{"id":"evince","name":"Evince","category":"Documents","packages":["evince"],"size_kb":15000,"exec":["sleep","30"],"desktop":"org.gnome.Evince.desktop","roles":["pdf"]},
{"id":"zathura","name":"Zathura","category":"Documents","packages":["zathura","zathura-pdf-mupdf"],"size_kb":1500,"exec":["sleep","30"],"desktop":"org.pwmt.zathura.desktop","roles":["pdf"]},
{"id":"font-x","name":"Font","category":"Fonts","packages":["font-x"],"size_kb":100,"exec":[]}
]}"#;

/// Protocol-level fake of walpad-pkg. Records its argv and its environment keys.
const FAKE_HELPER: &str = r#"#!/bin/sh
R=$WALPAD_PKG_ROOT
echo "$*" >> "$R/pkg-calls"
env | cut -d= -f1 | sort | tr '\n' ' ' > "$R/pkg-env"
op=$1
if [ "$op" = install ]; then shift 3; else shift 1; fi
id=$1; shift
if [ -e "$R/pkg-hang" ]; then exec sleep 30; fi
echo "PROGRESS 10 Updating the package index"
sleep 0.4
if [ -f "$R/pkg-fail" ]; then echo "ERROR net $(cat "$R/pkg-fail")"; exit 5; fi
echo "PROGRESS 60 Installing"
sleep 0.4
case $op in
  install) for p in "$@"; do echo "P:$p" >> "$R/lib/apk/db/installed"; done ;;
  remove) for p in "$@"; do grep -vx "P:$p" "$R/lib/apk/db/installed" > "$R/db.new"; mv "$R/db.new" "$R/lib/apk/db/installed"; done ;;
esac
echo "PROGRESS 100 Done"
echo OK
"#;

/// A tiny apk for the end-to-end test.
const FAKE_APK: &str = r#"#!/bin/sh
R=$WALPAD_PKG_ROOT
echo "$*" >> "$R/apk-calls"
cmd=$1; shift
case $cmd in
  update) exit 0 ;;
  add) n=$#; i=0; for p in "$@"; do i=$((i+1)); echo "($i/$n) Installing $p (1.0-r0)"; echo "P:$p" >> "$R/lib/apk/db/installed"; done ;;
  del) n=$#; i=0; for p in "$@"; do i=$((i+1)); echo "($i/$n) Purging $p (1.0-r0)"; grep -vx "P:$p" "$R/lib/apk/db/installed" > "$R/db.new"; mv "$R/db.new" "$R/lib/apk/db/installed"; done ;;
  info) [ "$1" = -r ] && echo "$2-1.0-r0 is required by:" ;;
esac
exit 0
"#;

struct Conn {
    w: UnixStream,
    r: BufReader<UnixStream>,
    events: Vec<Value>,
}
impl Conn {
    fn new(root: &Path) -> Conn {
        let s = UnixStream::connect(root.join("run/walpad/control.sock")).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut c = Conn {
            r: BufReader::new(s.try_clone().unwrap()),
            w: s,
            events: Vec::new(),
        };
        assert_eq!(
            c.call(1, "hello", json!({"client":"t","proto":1}))["ok"],
            true
        );
        assert_eq!(
            c.call(2, "subscribe", json!({"topics":["notice","status"]}))["ok"],
            true
        );
        c
    }
    fn recv(&mut self) -> Value {
        let mut l = String::new();
        self.r.read_line(&mut l).unwrap();
        serde_json::from_str(&l).unwrap()
    }
    fn call(&mut self, id: u64, cmd: &str, args: Value) -> Value {
        writeln!(self.w, "{}", json!({"v":1,"id":id,"cmd":cmd,"args":args})).unwrap();
        loop {
            let v = self.recv();
            if v["id"] == id {
                return v;
            }
            if v["event"] == "notice" {
                self.events.push(v["data"].clone());
            }
        }
    }
    /// Notices until `pkg_done`; returns all pkg notices seen since the last call.
    fn until_done(&mut self) -> Vec<Value> {
        let t = Instant::now();
        loop {
            if let Some(i) = self.events.iter().position(|e| e["kind"] == "pkg_done") {
                let mut seen: Vec<Value> = self.events.drain(..=i).collect();
                seen.retain(|e| e["kind"].as_str().is_some_and(|k| k.starts_with("pkg_")));
                return seen;
            }
            assert!(t.elapsed() < Duration::from_secs(15), "no pkg_done");
            let v = self.recv();
            if v["event"] == "notice" {
                self.events.push(v["data"].clone());
            }
        }
    }
    fn catalogue(&mut self, id: u64) -> Value {
        let r = self.call(id, "pkg_catalogue", Value::Null);
        assert_eq!(r["ok"], true, "{r}");
        r["data"].clone()
    }
    fn entry(&mut self, id: u64, app: &str) -> Value {
        self.catalogue(id)["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == app)
            .unwrap()
            .clone()
    }
    fn status_apps(&mut self, id: u64) -> Vec<String> {
        let r = self.call(id, "get_status", Value::Null);
        r["data"]["apps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["app"].as_str().unwrap().to_string())
            .collect()
    }
}

struct Rig {
    root: PathBuf,
    stop: Arc<AtomicBool>,
    h: Option<std::thread::JoinHandle<()>>,
}
impl Rig {
    /// `real`: run the shipped walpad-pkg and shipped catalogue against a fake apk.
    fn new(tag: &str, real: bool, tune: impl FnOnce(&mut Daemon)) -> Rig {
        let root = std::env::temp_dir().join(format!("walpad-pkgipc-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        fakeroot::populate(&root).unwrap();
        let w = |rel: &str, content: &str, mode: u32| {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, content).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
        };
        let cfg = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/rootfs");
        w("usr/bin/sleep", "", 0o644);
        w("lib/apk/db/installed", "P:busybox\n", 0o644);
        w(
            "etc/walpad/apps.json",
            r#"[{"id":"browser","name":"Browser","icon":"globe","exec":["sleep","30"]}]"#,
            0o644,
        );
        if real {
            let cat = std::fs::read_to_string(cfg.join("etc/walpad/catalogue.json")).unwrap();
            w("etc/walpad/catalogue.json", &cat, 0o644);
            let hp = std::fs::read_to_string(cfg.join("usr/local/sbin/walpad-pkg")).unwrap();
            w("usr/local/sbin/walpad-pkg", &hp, 0o755);
            w("sbin/apk", FAKE_APK, 0o755);
        } else {
            w("etc/walpad/catalogue.json", CATALOGUE, 0o644);
            w("usr/local/sbin/walpad-pkg", FAKE_HELPER, 0o755);
        }
        let mut d = Daemon::new(Root::new(&root)).unwrap();
        tune(&mut d);
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let h = std::thread::spawn(move || d.run(&s2));
        Rig {
            root,
            stop,
            h: Some(h),
        }
    }
    fn calls(&self) -> String {
        std::fs::read_to_string(self.root.join("pkg-calls")).unwrap_or_default()
    }
    fn apps_d(&self, id: &str) -> PathBuf {
        self.root.join(format!("var/lib/walpad/apps.d/{id}.json"))
    }
}
impl Drop for Rig {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.h.take() {
            let _ = h.join();
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn code(r: &Value) -> &str {
    r["error"]["code"].as_str().unwrap_or("")
}

#[test]
fn install_registers_the_app_after_apk_succeeds_and_uninstall_removes_it() {
    let rig = Rig::new("flow", false, |d| d.pkg_mut().set_reserve_kb(0));
    let mut c = Conn::new(&rig.root);
    let cat = c.catalogue(3);
    assert_eq!(cat["busy"], Value::Null);
    assert_eq!(cat["reserve_kb"], 0);
    assert!(cat["free_kb"].as_u64().unwrap() > 0);
    let e = c.entry(4, "mousepad");
    assert_eq!(
        (
            e["installed"].clone(),
            e["size_kb"].clone(),
            e["category"].clone()
        ),
        (json!(false), json!(700), json!("Documents"))
    );
    assert_eq!(c.status_apps(5), ["browser"]);

    let r = c.call(6, "pkg_install", json!({"id":"mousepad"}));
    assert_eq!(r["data"]["accepted"], true, "{r}");
    // one job at a time
    assert_eq!(
        code(&c.call(7, "pkg_install", json!({"id":"evince"}))),
        "busy"
    );
    assert_eq!(
        code(&c.call(8, "pkg_remove", json!({"id":"mousepad"}))),
        "busy"
    );
    let busy = c.catalogue(9)["busy"].clone();
    assert_eq!(busy["id"], "mousepad");
    assert_eq!(busy["op"], "install");
    assert!(
        !rig.apps_d("mousepad").exists(),
        "nothing is registered while apk is still working"
    );
    let seen = c.until_done();
    let pct: Vec<u64> = seen
        .iter()
        .filter(|e| e["kind"] == "pkg_progress")
        .map(|e| e["percent"].as_u64().unwrap())
        .collect();
    assert!(
        pct.len() >= 3 && pct.windows(2).all(|w| w[0] <= w[1]),
        "{pct:?}"
    );
    assert_eq!(*pct.last().unwrap(), 100);
    let done = seen.last().unwrap();
    assert_eq!(
        (&done["kind"], &done["op"], &done["id"], &done["ok"]),
        (
            &json!("pkg_done"),
            &json!("install"),
            &json!("mousepad"),
            &json!(true)
        ),
        "{done}"
    );
    // the helper got the catalogue's id and packages, never a client string, in a clean environment
    assert_eq!(
        rig.calls().trim(),
        "install --need-kb 700 mousepad mousepad"
    );
    let env = std::fs::read_to_string(rig.root.join("pkg-env")).unwrap();
    assert!(
        env.split_whitespace().all(|k| matches!(
            k,
            "PATH"
                | "PWD"
                | "SHLVL"
                | "_"
                | "WALPAD_PKG_ROOT"
                | "WALPAD_PKG_APK"
                | "WALPAD_PKG_RESERVE_KB"
                | "OLDPWD"
        )),
        "{env}"
    );
    // registered, installed, listed, and launchable
    let j: Value =
        serde_json::from_str(&std::fs::read_to_string(rig.apps_d("mousepad")).unwrap()).unwrap();
    assert_eq!(j["id"], "mousepad");
    assert_eq!(j["exec"], json!(["sleep", "30"]));
    assert_eq!(c.entry(10, "mousepad")["installed"], true);
    assert_eq!(c.status_apps(11), ["browser", "mousepad"]);
    assert_eq!(c.catalogue(12)["busy"], Value::Null);
    assert_eq!(
        c.call(13, "launch", json!({"app":"mousepad"}))["data"]["result"],
        "launched"
    );
    assert_eq!(
        code(&c.call(14, "pkg_install", json!({"id":"mousepad"}))),
        "refused",
        "already installed"
    );

    // uninstall while it runs: it is closed, the package and the entry go
    let r = c.call(15, "pkg_remove", json!({"id":"mousepad"}));
    assert_eq!(
        (&r["data"]["accepted"], &r["data"]["closed"]),
        (&json!(true), &json!(true)),
        "{r}"
    );
    let seen = c.until_done();
    assert_eq!(seen.last().unwrap()["ok"], true, "{seen:?}");
    assert_eq!(seen.last().unwrap()["op"], "remove");
    assert!(!rig.apps_d("mousepad").exists());
    assert_eq!(c.entry(16, "mousepad")["installed"], false);
    assert_eq!(c.status_apps(17), ["browser"]);
    assert_eq!(
        c.call(18, "launch", json!({"app":"mousepad"}))["error"]["code"],
        "invalid_args",
        "no longer in the allowlist"
    );
}

#[test]
fn a_failed_install_adds_no_entry_and_shows_the_error() {
    let rig = Rig::new("fail", false, |d| d.pkg_mut().set_reserve_kb(0));
    std::fs::write(
        rig.root.join("pkg-fail"),
        "Could not reach the package repository",
    )
    .unwrap();
    let mut c = Conn::new(&rig.root);
    assert_eq!(
        c.call(3, "pkg_install", json!({"id":"mousepad"}))["ok"],
        true
    );
    let seen = c.until_done();
    let done = seen.last().unwrap();
    assert_eq!(done["ok"], false, "{done}");
    assert!(
        done["error"].as_str().unwrap().contains("Could not reach"),
        "{done}"
    );
    assert!(!rig.apps_d("mousepad").exists(), "no apps.d entry");
    assert!(!rig
        .root
        .join("var/lib/walpad/apps.d")
        .join("mousepad.json")
        .exists());
    assert_eq!(c.status_apps(4), ["browser"]);
    assert_eq!(c.entry(5, "mousepad")["installed"], false);
    // and the next attempt works
    std::fs::remove_file(rig.root.join("pkg-fail")).unwrap();
    assert_eq!(
        c.call(6, "pkg_install", json!({"id":"mousepad"}))["ok"],
        true
    );
    assert_eq!(c.until_done().last().unwrap()["ok"], true);
}

#[test]
fn a_hanging_install_is_killed_at_the_deadline() {
    let rig = Rig::new("hang", false, |d| {
        d.pkg_mut().set_reserve_kb(0);
        d.pkg_mut()
            .set_timeouts(Duration::from_millis(700), Duration::from_millis(700));
    });
    std::fs::write(rig.root.join("pkg-hang"), "").unwrap();
    let mut c = Conn::new(&rig.root);
    assert_eq!(
        c.call(3, "pkg_install", json!({"id":"mousepad"}))["ok"],
        true
    );
    let t = Instant::now();
    let seen = c.until_done();
    assert!(t.elapsed() < Duration::from_secs(5));
    let done = seen.last().unwrap();
    assert_eq!(done["ok"], false);
    assert!(
        done["error"].as_str().unwrap().contains("Timed out"),
        "{done}"
    );
    assert!(!rig.apps_d("mousepad").exists());
    assert_eq!(
        c.catalogue(4)["busy"],
        Value::Null,
        "the slot is free again"
    );
}

#[test]
fn install_is_refused_below_the_space_reserve_and_nothing_runs() {
    // the host's free space stands in for "4 GiB free": the reserve is set just above it
    let rig = Rig::new("space", false, |d| {
        let (_, free) = walpad_hw::sysinfo::disk_usage(&Root::new(std::env::temp_dir())).unwrap();
        d.pkg_mut().set_reserve_kb(free / 1024 + 1024);
    });
    let mut c = Conn::new(&rig.root);
    let r = c.call(3, "pkg_install", json!({"id":"mousepad"}));
    assert_eq!(code(&r), "refused", "{r}");
    let msg = r["error"]["msg"].as_str().unwrap();
    assert!(
        msg.contains("Not enough free space") && msg.contains("must stay free"),
        "{msg}"
    );
    assert_eq!(rig.calls(), "", "the helper was not started");
    assert_eq!(c.entry(4, "mousepad")["installed"], false);
    assert_eq!(c.status_apps(5), ["browser"]);
}

#[test]
fn only_catalogue_entries_and_never_system_apps() {
    let rig = Rig::new("refuse", false, |d| d.pkg_mut().set_reserve_kb(0));
    let mut c = Conn::new(&rig.root);
    for (cmd, id, want) in [
        ("pkg_install", "browser", "refused"),
        ("pkg_remove", "browser", "refused"),
        ("pkg_install", "nope", "invalid_args"),
        ("pkg_remove", "nope", "invalid_args"),
        ("pkg_install", "--allow-untrusted", "invalid_args"),
        ("pkg_install", "/tmp/x.apk", "invalid_args"),
        ("pkg_install", "firefox-esr", "invalid_args"),
        ("pkg_remove", "evince", "refused"),
    ] {
        let r = c.call(10, cmd, json!({"id": id}));
        assert_eq!(code(&r), want, "{cmd} {id}: {r}");
    }
    assert_eq!(
        c.call(
            11,
            "pkg_install",
            json!({"id":"mousepad","packages":["evil"]})
        )["ok"],
        true,
        "extra fields are ignored"
    );
    c.until_done();
    assert_eq!(
        rig.calls().trim(),
        "install --need-kb 700 mousepad mousepad",
        "no client package name reaches the helper"
    );
}

#[test]
fn default_apps_are_recorded_and_applied_to_mimeapps() {
    let rig = Rig::new("defaults", false, |_| {});
    std::fs::write(
        rig.root.join("lib/apk/db/installed"),
        "P:evince\nP:zathura\nP:zathura-pdf-mupdf\n",
    )
    .unwrap();
    let mut c = Conn::new(&rig.root);
    let cat = c.catalogue(3);
    assert_eq!(cat["defaults"]["pdf"], "evince", "first installed viewer");
    assert_eq!(cat["defaults"]["browser"], "browser");
    let r = c.call(4, "default_app", json!({"role":"pdf","app":"zathura"}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(c.catalogue(5)["defaults"]["pdf"], "zathura");
    let d: Value = serde_json::from_str(
        &std::fs::read_to_string(rig.root.join("etc/walpad/defaults.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(d["roles"]["pdf"], "zathura");
    let m = std::fs::read_to_string(rig.root.join("root/.config/mimeapps.list")).unwrap();
    assert!(
        m.contains("application/pdf=org.pwmt.zathura.desktop;"),
        "{m}"
    );
    for (role, app, want) in [
        ("pdf", "mousepad", "refused"),
        ("pdf", "font-x", "refused"),
        ("pdf", "nope", "invalid_args"),
        ("shell", "evince", "invalid_args"),
        ("pdf", "../evince", "invalid_args"),
    ] {
        let r = c.call(6, "default_app", json!({"role": role, "app": app}));
        assert_eq!(code(&r), want, "{role} {app}: {r}");
    }
}

#[test]
fn shipped_helper_and_catalogue_end_to_end_with_a_fake_apk() {
    let rig = Rig::new("e2e", true, |d| d.pkg_mut().set_reserve_kb(0));
    let mut c = Conn::new(&rig.root);
    assert_eq!(c.entry(3, "mousepad")["installed"], false);
    assert_eq!(
        c.call(4, "pkg_install", json!({"id":"mousepad"}))["ok"],
        true
    );
    let seen = c.until_done();
    assert_eq!(seen.last().unwrap()["ok"], true, "{seen:?}");
    assert!(
        seen.iter().any(|e| e["kind"] == "pkg_progress"
            && e["text"]
                .as_str()
                .is_some_and(|t| t.starts_with("Installing mousepad"))),
        "apk's own progress line is passed on: {seen:?}"
    );
    let apk = std::fs::read_to_string(rig.root.join("apk-calls")).unwrap();
    assert_eq!(apk.trim(), "update\nadd mousepad", "{apk}");
    assert!(!apk.contains("allow-untrusted"));
    assert!(rig.apps_d("mousepad").is_file());
    assert_eq!(c.entry(5, "mousepad")["installed"], true);
    // multi-package entry
    assert_eq!(
        c.call(6, "pkg_install", json!({"id":"zathura"}))["ok"],
        true
    );
    assert_eq!(c.until_done().last().unwrap()["ok"], true);
    assert!(std::fs::read_to_string(rig.root.join("apk-calls"))
        .unwrap()
        .contains("add zathura zathura-pdf-mupdf"));
    // uninstall
    assert_eq!(
        c.call(7, "pkg_remove", json!({"id":"mousepad"}))["ok"],
        true
    );
    let seen = c.until_done();
    assert_eq!(seen.last().unwrap()["ok"], true, "{seen:?}");
    assert!(!rig.apps_d("mousepad").exists());
    assert_eq!(c.entry(8, "mousepad")["installed"], false);
    // the helper refuses a system package even if the core were asked to: built-in entries
    // are refused before it runs
    assert_eq!(
        code(&c.call(9, "pkg_remove", json!({"id":"terminal"}))),
        "refused"
    );
}
