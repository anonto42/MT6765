use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use walpad_core::{daemon::Daemon, fake};

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[test]
fn key_held_exit_status() {
    let root = std::env::temp_dir().join(format!("walpad-ctl-key-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let r = Root::new(&root);
    let a = s(&["key-held", "volumedown"]);
    assert_eq!(key_held_status(&a, &r), 2); // no device
    std::fs::create_dir_all(root.join("sys/class/input/input2/event1")).unwrap();
    std::fs::write(root.join("sys/class/input/input2/name"), "mtk-kpd\n").unwrap();
    std::fs::create_dir_all(root.join("dev/input")).unwrap();
    std::fs::write(root.join("dev/input/event1"), vec![0u8; 96]).unwrap();
    assert_eq!(key_held_status(&a, &r), 1);
    let mut b = vec![0u8; 96];
    b[14] = 4;
    std::fs::write(root.join("dev/input/event1"), b).unwrap();
    assert_eq!(key_held_status(&a, &r), 0);
    assert_eq!(key_held_status(&s(&["key-held", "114"]), &r), 0);
    assert_eq!(key_held_status(&s(&["key-held", "115"]), &r), 1); // same device, not held
    assert_eq!(key_held_status(&s(&["key-held", "116"]), &r), 2); // no mtk-pmic-keys here
    assert_eq!(key_held_status(&s(&["key-held", "bogus"]), &r), 2);
    assert_eq!(key_held_status(&s(&["key-held"]), &r), 2);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn timeout_cmd_runs_and_kills_without_orphans() {
    assert_eq!(timeout_cmd(&s(&["2", "true"])), 0);
    assert_eq!(timeout_cmd(&s(&["2", "false"])), 1);
    assert_eq!(timeout_cmd(&s(&["-k", "1", "0.1", "sleep", "5"])), 124);
    assert_eq!(timeout_cmd(&s(&["-s", "KILL", "0.1", "sleep", "5"])), 137);
    assert_eq!(timeout_cmd(&s(&["2", "definitely-missing-prog-xyz"])), 127);
}

#[test]
fn lock_clear_removes_the_lock_without_a_core_and_without_a_pin() {
    let root = std::env::temp_dir().join(format!("walpad-ctl-lock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("var/lib/walpad")).unwrap();
    std::fs::create_dir_all(root.join("run/walpad")).unwrap();
    std::fs::write(root.join("var/lib/walpad/lock.json"), "{}").unwrap();
    std::fs::write(root.join("run/walpad/locked"), "1").unwrap();
    let r = Root::new(&root);
    assert_eq!(run(&s(&["lock_clear"]), &r).unwrap(), "screen lock removed");
    assert!(!root.join("var/lib/walpad/lock.json").exists());
    assert!(!root.join("run/walpad/locked").exists());
    assert_eq!(
        run(&s(&["lock-clear"]), &r).unwrap(),
        "no screen lock was set"
    );
    assert_eq!(
        build_request(&s(&["lock-now"])).unwrap(),
        ("lock_now".into(), Value::Null)
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn request_building() {
    assert_eq!(
        build_request(&s(&["brightness", "+30"])).unwrap(),
        ("set_brightness".into(), json!({"delta": 30}))
    );
    assert_eq!(
        build_request(&s(&["brightness", "90"])).unwrap().1,
        json!({"value": 90})
    );
    assert_eq!(
        build_request(&s(&["rotate", "toggle"])).unwrap().1,
        json!({"mode": "toggle"})
    );
    assert_eq!(
        build_request(&s(&["power", "poweroff"])).unwrap().1["confirmed"],
        false
    );
    assert_eq!(
        build_request(&s(&["power", "poweroff", "--yes"]))
            .unwrap()
            .1["confirmed"],
        true
    );
    assert!(build_request(&s(&["brightness", "x"])).is_err());
    assert_eq!(
        build_request(&s(&["volume", "+5"])).unwrap(),
        ("volume_delta".into(), json!({"delta": 5}))
    );
    assert_eq!(
        build_request(&s(&["volume", "40"])).unwrap(),
        ("volume_set".into(), json!({"percent": 40}))
    );
    assert_eq!(
        build_request(&s(&["mute", "on"])).unwrap(),
        ("mute".into(), json!({"on": true}))
    );
    assert!(build_request(&s(&["mute", "maybe"])).is_err());
    assert_eq!(
        build_request(&s(&["jack", "headphones"])).unwrap(),
        ("jack_force".into(), json!({"route": "headphones"}))
    );
    assert_eq!(
        build_request(&s(&["jack", "headset"])).unwrap(),
        ("jack_force".into(), json!({"route": "headset_with_mic"}))
    );
    assert_eq!(
        build_request(&s(&["jack", "off"])).unwrap(),
        ("jack_force".into(), json!({"route": null}))
    );
    assert!(build_request(&s(&["jack", "nonsense"])).is_err());
    assert!(build_request(&s(&["volume", "loud"])).is_err());
    assert!(build_request(&s(&["screen"])).is_err());
    assert!(build_request(&s(&["nonsense"])).is_err());
    assert_eq!(
        build_request(&s(&["wifi", "off"])).unwrap(),
        ("wifi_enable".into(), json!({"on": false}))
    );
    assert_eq!(
        build_request(&s(&["hotspot", "on"])).unwrap(),
        ("hotspot".into(), json!({"on": true}))
    );
    assert!(build_request(&s(&["hotspot", "maybe"])).is_err());
    assert_eq!(
        build_request(&s(&["gps", "on"])).unwrap(),
        ("gps".into(), json!({"on": true}))
    );
    assert!(build_request(&s(&["gps"])).is_err());
    assert_eq!(
        build_request(&s(&["stats", "net", "off"])).unwrap(),
        ("stats_config".into(), json!({"net": false}))
    );
    assert!(build_request(&s(&["stats", "gpu", "on"])).is_err());
    assert!(build_request(&s(&["stats", "cpu"])).is_err());
    assert_eq!(
        build_request(&s(&["vpn", "on"])).unwrap(),
        ("vpn".into(), json!({"on": true}))
    );
    assert_eq!(
        build_request(&s(&["vpn", "dns", "1.1.1.1", "9.9.9.9"])).unwrap(),
        ("vpn_config".into(), json!({"dns": ["1.1.1.1", "9.9.9.9"]}))
    );
    assert_eq!(
        build_request(&s(&["vpn", "proxy", "clear"])).unwrap(),
        ("vpn_config".into(), json!({"proxy": ""}))
    );
    assert_eq!(
        build_request(&s(&["vpn", "kill-switch", "on"])).unwrap(),
        ("vpn_config".into(), json!({"kill_switch": true}))
    );
    assert!(build_request(&s(&["vpn", "dns"])).is_err());
    assert!(build_request(&s(&["vpn", "bogus"])).is_err());
}

#[test]
fn talks_to_fake_core_and_falls_back_without_it() {
    let root = std::env::temp_dir().join(format!("walpad-ctl-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    fake::populate(&root).unwrap();
    let r = Root::new(&root);
    // legacy fallback with no core: status reads status.json, commands go to the FIFO
    std::fs::create_dir_all(root.join("run/walpad")).unwrap();
    std::fs::write(root.join("run/walpad/status.json"), "{\"legacy\":true}").unwrap();
    assert!(run(&s(&["status"]), &r).unwrap().contains("legacy"));
    assert!(run(&s(&["wifi-saved"]), &r)
        .unwrap_err()
        .contains("not running"));
    std::fs::write(root.join("run/walpad/devd.cmd"), "").unwrap();
    run(&s(&["rotate", "lock"]), &r).unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("run/walpad/devd.cmd"))
            .unwrap()
            .trim(),
        "rotate lock"
    );
    std::fs::remove_file(root.join("run/walpad/devd.cmd")).unwrap();

    let mut d = Daemon::new(Root::new(&root)).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let s2 = stop.clone();
    let h = std::thread::spawn(move || d.run(&s2));
    let out = run(&s(&["status"]), &r).unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["battery"]["source"], "voltage_estimate");
    assert!(run(&s(&["brightness", "+20"]), &r).unwrap().contains("140"));
    assert!(run(&s(&["power", "reboot"]), &r)
        .unwrap_err()
        .contains("--yes"));
    // Wi-Fi through the core with a fake supplicant: scan prints networks, connect reads the passphrase from stdin
    std::fs::create_dir_all(root.join("etc/wpa_supplicant")).unwrap();
    let _sup =
        walpad_core::wpa::fake::spawn(&root, walpad_core::wpa::fake::default_aps(), None).unwrap();
    let t = std::time::Instant::now();
    let scan = loop {
        match run(&s(&["wifi-scan"]), &r) {
            Ok(t) => break t,
            Err(e) if t.elapsed() < Duration::from_secs(5) && e.contains("unavailable") => {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(e) => panic!("wifi-scan failed: {e}"),
        }
    };
    assert!(
        scan.contains("Home") && scan.contains("wpa") && scan.contains("Cafe Open"),
        "{scan}"
    );
    let mut pw = std::io::Cursor::new(b"correct horse\n".to_vec());
    assert_eq!(
        run_with_stdin(&s(&["wifi-connect", "Home"]), &r, &mut pw).unwrap(),
        "connected to Home"
    );
    let mut wrong = std::io::Cursor::new(b"definitely wrong\n".to_vec());
    assert!(
        run_with_stdin(&s(&["wifi-connect", "Home"]), &r, &mut wrong)
            .unwrap_err()
            .contains("wrong_password")
    );
    let mut none = std::io::Cursor::new(Vec::new());
    assert!(run_with_stdin(&s(&["wifi-connect", "Home"]), &r, &mut none)
        .unwrap_err()
        .contains("no passphrase"));
    assert!(run_with_stdin(
        &s(&["wifi-connect", "Cafe Open", "--open"]),
        &r,
        &mut std::io::Cursor::new(Vec::new())
    )
    .unwrap()
    .contains("connected"));
    stop.store(true, Ordering::Relaxed);
    h.join().unwrap();
    let _ = std::fs::remove_dir_all(&root);
}
