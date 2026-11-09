//! Wi-Fi manager against the fake supplicant over real datagram sockets.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use walpad_core::wpa::fake::{self, FakeAp, FakeSupplicant};
use walpad_core::wpa::manager::{Wifi, WifiError, WifiEvent};
use walpad_core::wpa::parse::{hex, ssid_display};
use walpad_core::wpa::psk;
use walpad_hw::Root;
use walpad_proto::Secret;

struct Env {
    root: PathBuf,
    conf: PathBuf,
    sup: Option<FakeSupplicant>,
}
fn env(name: &str, aps: Vec<FakeAp>) -> Env {
    let root = std::env::temp_dir().join(format!("walpad-wifi-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("run/walpad")).unwrap();
    std::fs::create_dir_all(root.join("etc/wpa_supplicant")).unwrap();
    let conf = root.join("etc/wpa_supplicant/wpa_supplicant.conf");
    let sup = fake::spawn(&root, aps, Some(conf.clone())).unwrap();
    Env {
        root,
        conf,
        sup: Some(sup),
    }
}
impl Env {
    fn wifi(&self) -> Wifi {
        let mut w = Wifi::new(&Root::new(&self.root));
        w.set_timing(Duration::from_millis(1200), Duration::from_millis(100));
        w
    }
    fn log(&self) -> Vec<String> {
        self.sup.as_ref().unwrap().log.lock().unwrap().clone()
    }
}
impl Drop for Env {
    fn drop(&mut self) {
        self.sup = None;
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Tick until `pred` matches an event (5 s max).
fn until(w: &mut Wifi, mut pred: impl FnMut(&WifiEvent) -> bool) -> WifiEvent {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(5) {
        for e in w.tick(Instant::now()) {
            if pred(&e) {
                return e;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for a wifi event");
}
fn wait_connected(w: &mut Wifi) {
    let t = Instant::now();
    while !w.connected() && t.elapsed() < Duration::from_secs(3) {
        w.tick(Instant::now());
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(w.connected(), "never connected to the control socket");
}
fn secret(s: &str) -> Secret {
    Secret::new(s)
}

#[test]
fn unavailable_without_a_supplicant() {
    let root = std::env::temp_dir().join(format!("walpad-wifi-{}-none", std::process::id()));
    std::fs::create_dir_all(root.join("run/walpad")).unwrap();
    let mut w = Wifi::new(&Root::new(&root));
    assert_eq!(w.scan(Instant::now()), Err(WifiError::Unavailable));
    assert!(w.tick(Instant::now()).is_empty());
    assert!(!w.connected());
}

#[test]
fn scan_is_single_flight_merged_sorted_and_utf8_safe() {
    let e = env("scan", fake::default_aps());
    let mut w = e.wifi();
    wait_connected(&mut w);
    w.scan(Instant::now()).unwrap();
    assert_eq!(
        w.scan(Instant::now()),
        Err(WifiError::Busy),
        "second scan while scanning"
    );
    let WifiEvent::Scan { networks } = until(&mut w, |e| matches!(e, WifiEvent::Scan { .. }))
    else {
        unreachable!()
    };
    let names: Vec<String> = networks.iter().map(|n| ssid_display(&n.ssid)).collect();
    assert_eq!(
        names,
        vec!["Home", "Cafe Open", "Café"],
        "hidden SSID dropped, duplicate Home merged (-52 kept), sorted by signal"
    );
    assert_eq!(networks[0].signal_dbm, -52);
    assert_eq!(networks[1].security.as_str(), "open");
    // idle again: another scan is allowed
    assert!(w.scan(Instant::now()).is_ok());
}

#[test]
fn connect_wpa2_derives_psk_never_sends_the_passphrase_and_saves_with_mode_600() {
    let e = env("wpa2", fake::default_aps());
    let mut w = e.wifi();
    wait_connected(&mut w);
    w.connect(
        b"Home".to_vec(),
        Some(&secret("correct horse")),
        None,
        Instant::now(),
    )
    .unwrap();
    assert_eq!(
        w.connect(b"Home".to_vec(), None, None, Instant::now()),
        Err(WifiError::Busy)
    );
    let ev = until(&mut w, |e| matches!(e, WifiEvent::Connect { .. }));
    assert_eq!(
        ev,
        WifiEvent::Connect {
            ok: true,
            error: None,
            ssid: b"Home".to_vec()
        }
    );

    let log = e.log();
    let joined = log.join("\n");
    assert!(
        !joined.contains("correct horse"),
        "passphrase must never be sent"
    );
    let want = format!(
        "SET_NETWORK 1 psk {}",
        hex(&psk::derive("correct horse", b"Home"))
    );
    assert!(log.contains(&want), "derived PSK sent: {log:?}");
    assert!(log.contains(&format!("SET_NETWORK 1 ssid {}", hex(b"Home"))));
    assert!(log.contains(&"SELECT_NETWORK 1".to_string()));
    assert!(
        log.contains(&"ENABLE_NETWORK 0".to_string()),
        "other saved networks re-enabled (they auto-connect again)"
    );
    assert!(log.contains(&"SAVE_CONFIG".to_string()));
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(&e.conf).unwrap().permissions().mode() & 0o777,
        0o600,
        "config forced back to 0600"
    );

    // live status after connecting
    std::thread::sleep(Duration::from_millis(100));
    for _ in 0..30 {
        w.tick(Instant::now() + Duration::from_secs(10));
        if w.live().signal_dbm.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    assert_eq!(w.live().ssid.as_deref(), Some("Home"));
    assert_eq!(w.live().ip.as_deref(), Some("192.168.0.77"));
    assert_eq!(w.live().signal_dbm, Some(-58));
}

#[test]
fn wrong_password_is_detected_and_cleaned_up() {
    let e = env("wrongkey", fake::default_aps());
    let mut w = e.wifi();
    wait_connected(&mut w);
    w.connect(
        b"Home".to_vec(),
        Some(&secret("not the password")),
        None,
        Instant::now(),
    )
    .unwrap();
    let ev = until(&mut w, |e| matches!(e, WifiEvent::Connect { .. }));
    assert_eq!(
        ev,
        WifiEvent::Connect {
            ok: false,
            error: Some("wrong_password"),
            ssid: b"Home".to_vec()
        }
    );
    let log = e.log();
    assert!(
        log.contains(&"REMOVE_NETWORK 1".to_string()),
        "failed new network is removed"
    );
    assert!(
        log.contains(&"ENABLE_NETWORK 0".to_string()),
        "others re-enabled"
    );
    assert!(
        !log.contains(&"SAVE_CONFIG".to_string()),
        "nothing saved on failure"
    );
    assert!(!log.join("\n").contains("not the password"));
    assert_eq!(w.list_saved().unwrap().len(), 1);
}

#[test]
fn open_network_uses_key_mgmt_none_and_timeout_is_reported() {
    let mut aps = fake::default_aps();
    aps.push(FakeAp {
        ssid: b"Silent".to_vec(),
        signal: -50,
        freq: 2412,
        flags: "[WPA2-PSK-CCMP][ESS]".into(),
        passphrase: Some("whatever123".into()),
    });
    let e = env("open", aps);
    let mut w = e.wifi();
    wait_connected(&mut w);
    w.connect(b"Cafe Open".to_vec(), None, None, Instant::now())
        .unwrap();
    assert_eq!(
        until(&mut w, |e| matches!(e, WifiEvent::Connect { .. })),
        WifiEvent::Connect {
            ok: true,
            error: None,
            ssid: b"Cafe Open".to_vec()
        }
    );
    assert!(e.log().iter().any(|l| l.ends_with("key_mgmt NONE")));
    // a network that never answers: timeout after the (shortened) connect timeout, then removed
    w.connect(b"Nowhere".to_vec(), None, None, Instant::now())
        .unwrap();
    let ev = until(&mut w, |e| matches!(e, WifiEvent::Connect { .. }));
    assert_eq!(
        ev,
        WifiEvent::Connect {
            ok: false,
            error: Some("timeout"),
            ssid: b"Nowhere".to_vec()
        }
    );
}

#[test]
fn saved_list_forget_disconnect_and_validation() {
    let e = env("saved", fake::default_aps());
    let mut w = e.wifi();
    wait_connected(&mut w);
    let saved = w.list_saved().unwrap();
    assert_eq!(
        (saved[0].id, ssid_display(&saved[0].ssid)),
        (0, "Existing".to_string())
    );
    // invalid secrets are rejected before the supplicant is touched
    let before = e.log().len();
    assert!(
        matches!(w.connect(b"Home".to_vec(), Some(&secret("12345")), None, Instant::now()), Err(WifiError::Invalid(m)) if m.contains("8-63"))
    );
    assert!(matches!(
        w.connect(
            b"Home".to_vec(),
            Some(&secret("short77")),
            None,
            Instant::now()
        ),
        Err(WifiError::Invalid(_))
    ));
    assert!(matches!(
        w.connect(Vec::new(), None, None, Instant::now()),
        Err(WifiError::Invalid(_))
    ));
    assert!(matches!(
        w.connect(Vec::new(), None, Some(99), Instant::now()),
        Err(WifiError::Invalid(_))
    ));
    assert!(
        !e.log()[before..]
            .iter()
            .any(|l| l.starts_with("ADD_NETWORK") || l.starts_with("SET_NETWORK")),
        "no supplicant change for invalid input"
    );
    w.disconnect().unwrap();
    w.forget(0).unwrap();
    assert!(w.list_saved().unwrap().is_empty());
    assert!(e.log().contains(&"SAVE_CONFIG".to_string()));
    assert!(matches!(w.forget(0), Err(WifiError::Failed(_))));
}

#[test]
fn reconnects_when_the_supplicant_restarts() {
    let mut e = env("restart", fake::default_aps());
    let mut w = e.wifi();
    wait_connected(&mut w);
    e.sup = None; // supplicant dies
    let _ = w.scan(Instant::now());
    assert!(!w.connected(), "request failure drops the client");
    assert_eq!(w.scan(Instant::now()), Err(WifiError::Unavailable));
    e.sup = Some(fake::spawn(&e.root, fake::default_aps(), Some(e.conf.clone())).unwrap());
    wait_connected(&mut w);
    assert!(w.scan(Instant::now()).is_ok());
    let _ = Path::new("/");
}
