//! A tiny fake wpa_supplicant (control interface) for tests and `walpad-core --fake`. It speaks the
//! same datagram protocol, keeps a network table and simulates scanning, WPA2 key checks and the
//! wrong-key event. It records every command (tests assert no passphrase ever appears in them).
use super::parse::{hex, unhex};
use super::psk;
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod aps;

use aps::Net;
pub use aps::{default_aps, FakeAp};

pub struct FakeSupplicant {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
    pub log: Arc<Mutex<Vec<String>>>,
    sock: PathBuf,
}

impl Drop for FakeSupplicant {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        let _ = std::fs::remove_file(&self.sock);
    }
}

/// Start a fake supplicant listening at `<root>/run/wpa_supplicant/wlan0`; `conf` is the path of the
/// config file SAVE_CONFIG writes (mode 0644 on purpose, so the manager's chmod is exercised).
pub fn spawn(
    root: &Path,
    aps: Vec<FakeAp>,
    conf: Option<PathBuf>,
) -> std::io::Result<FakeSupplicant> {
    let dir = root.join("run/wpa_supplicant");
    std::fs::create_dir_all(&dir)?;
    let sock = dir.join("wlan0");
    let _ = std::fs::remove_file(&sock);
    let s = UnixDatagram::bind(&sock)?;
    s.set_read_timeout(Some(Duration::from_millis(30)))?;
    let stop = Arc::new(AtomicBool::new(false));
    let log = Arc::new(Mutex::new(Vec::new()));
    let (stop2, log2) = (stop.clone(), log.clone());
    let handle = std::thread::spawn(move || serve(s, aps, conf, stop2, log2));
    Ok(FakeSupplicant {
        stop,
        handle: Some(handle),
        log,
        sock,
    })
}

fn serve(
    s: UnixDatagram,
    aps: Vec<FakeAp>,
    conf: Option<PathBuf>,
    stop: Arc<AtomicBool>,
    log: Arc<Mutex<Vec<String>>>,
) {
    let mut nets: Vec<Net> = vec![Net {
        id: 0,
        ssid_hex: hex(b"Existing"),
        psk: Some("00".repeat(32)),
        open: false,
        enabled: true,
    }];
    let mut next_id = 1u32;
    let mut attached: Vec<PathBuf> = Vec::new();
    let mut pending: Vec<(Instant, String)> = Vec::new();
    let mut state = "DISCONNECTED".to_string();
    let mut current: Option<u32> = None;
    let send_to = |s: &UnixDatagram, p: &Path, m: &str| {
        let _ = s.send_to(m.as_bytes(), p);
    };
    let mut buf = vec![0u8; 8192];
    while !stop.load(Ordering::Relaxed) {
        let now = Instant::now();
        let mut due = Vec::new();
        pending.retain(|(t, m)| {
            if *t <= now {
                due.push(m.clone());
                false
            } else {
                true
            }
        });
        for m in due {
            if let Some(rest) = m.strip_prefix("@state:") {
                state = rest.to_string();
                continue;
            }
            for a in &attached {
                send_to(&s, a, &m);
            }
        }
        let Ok((n, from)) = s.recv_from(&mut buf) else {
            continue;
        };
        let Some(peer) = from.as_pathname().map(|p| p.to_path_buf()) else {
            continue;
        };
        let cmd = String::from_utf8_lossy(&buf[..n]).into_owned();
        log.lock().map(|mut l| l.push(cmd.clone())).ok();
        let mut w = cmd.splitn(4, ' ');
        let verb = w.next().unwrap_or("");
        let reply: String = match verb {
            "PING" => "PONG\n".into(),
            "ATTACH" => {
                attached.push(peer.clone());
                "OK\n".into()
            }
            "SCAN" => {
                pending.push((
                    now + Duration::from_millis(150),
                    "<3>CTRL-EVENT-SCAN-RESULTS ".into(),
                ));
                "OK\n".into()
            }
            "SCAN_RESULTS" => {
                let mut t = String::from("bssid / frequency / signal level / flags / ssid\n");
                for (i, ap) in aps.iter().enumerate() {
                    let ssid: String = ap
                        .ssid
                        .iter()
                        .map(|&b| {
                            if b.is_ascii_graphic() || b == b' ' {
                                (b as char).to_string()
                            } else {
                                format!("\\x{b:02x}")
                            }
                        })
                        .collect();
                    t.push_str(&format!(
                        "02:00:00:00:00:{i:02x}\t{}\t{}\t{}\t{ssid}\n",
                        ap.freq, ap.signal, ap.flags
                    ));
                }
                t.push_str("02:00:00:00:00:ff\t2412\t-30\t[WPA2-PSK-CCMP][ESS]\t\n");
                t
            }
            "LIST_NETWORKS" => {
                let mut t = String::from("network id / ssid / bssid / flags\n");
                for n in &nets {
                    let ssid = unhex(&n.ssid_hex)
                        .map(|b| String::from_utf8_lossy(&b).into_owned())
                        .unwrap_or_default();
                    let flag = if current == Some(n.id) {
                        "[CURRENT]"
                    } else if !n.enabled {
                        "[DISABLED]"
                    } else {
                        ""
                    };
                    t.push_str(&format!("{}\t{ssid}\tany\t{flag}\n", n.id));
                }
                t
            }
            "ADD_NETWORK" => {
                nets.push(Net {
                    id: next_id,
                    ssid_hex: String::new(),
                    psk: None,
                    open: false,
                    enabled: true,
                });
                next_id += 1;
                format!("{}\n", next_id - 1)
            }
            "SET_NETWORK" => {
                let rest: Vec<&str> = cmd.splitn(4, ' ').collect();
                let (Some(id), Some(key), Some(val)) = (
                    rest.get(1).and_then(|x| x.parse::<u32>().ok()),
                    rest.get(2),
                    rest.get(3),
                ) else {
                    send_to(&s, &peer, "FAIL\n");
                    continue;
                };
                match nets.iter_mut().find(|n| n.id == id) {
                    Some(n) => {
                        match *key {
                            "ssid" => n.ssid_hex = val.to_string(),
                            "psk" => n.psk = Some(val.to_string()),
                            "key_mgmt" => n.open = *val == "NONE",
                            _ => {}
                        }
                        "OK\n".into()
                    }
                    None => "FAIL\n".into(),
                }
            }
            "REMOVE_NETWORK" => {
                let id: u32 = w.next().and_then(|x| x.parse().ok()).unwrap_or(u32::MAX);
                let before = nets.len();
                nets.retain(|n| n.id != id);
                if nets.len() < before {
                    "OK\n".into()
                } else {
                    "FAIL\n".into()
                }
            }
            "ENABLE_NETWORK" => {
                let a = w.next().unwrap_or("");
                for n in nets.iter_mut() {
                    if a == "all" || a.parse() == Ok(n.id) {
                        n.enabled = true;
                    }
                }
                "OK\n".into()
            }
            "SELECT_NETWORK" => {
                let id: u32 = w.next().and_then(|x| x.parse().ok()).unwrap_or(u32::MAX);
                let Some((net_ssid_hex, net_open, net_psk)) = nets
                    .iter()
                    .find(|n| n.id == id)
                    .map(|n| (n.ssid_hex.clone(), n.open, n.psk.clone()))
                else {
                    send_to(&s, &peer, "FAIL\n");
                    continue;
                };
                for n in nets.iter_mut() {
                    n.enabled = n.id == id;
                }
                let ssid = unhex(&net_ssid_hex).unwrap_or_default();
                let ap = aps.iter().find(|a| a.ssid == ssid);
                let ok = match ap {
                    None => false,
                    Some(ap) => match (&ap.passphrase, net_open) {
                        (None, _) => true,
                        (Some(pw), false) => {
                            net_psk.as_deref() == Some(hex(&psk::derive(pw, &ap.ssid)).as_str())
                        }
                        (Some(_), true) => false,
                    },
                };
                if ok {
                    current = Some(id);
                    pending.push((
                        now + Duration::from_millis(200),
                        "@state:COMPLETED".to_string(),
                    ));
                    pending.push((now + Duration::from_millis(210), format!("<3>CTRL-EVENT-CONNECTED - Connection to 02:00:00:00:00:00 completed [id={id} id_str=]")));
                } else if ap.is_some() {
                    pending.push((now + Duration::from_millis(200), format!("<3>CTRL-EVENT-SSID-TEMP-DISABLED id={id} ssid=\"x\" auth_failures=1 duration=10 reason=WRONG_KEY")));
                }
                "OK\n".into()
            }
            "DISCONNECT" => {
                current = None;
                state = "DISCONNECTED".into();
                pending.push((
                    now + Duration::from_millis(10),
                    "<3>CTRL-EVENT-DISCONNECTED bssid=02:00:00:00:00:00 reason=3".into(),
                ));
                "OK\n".into()
            }
            "STATUS" => {
                let mut t = String::new();
                if let Some(id) = current {
                    let ssid = nets
                        .iter()
                        .find(|n| n.id == id)
                        .and_then(|n| unhex(&n.ssid_hex))
                        .map(|b| String::from_utf8_lossy(&b).into_owned())
                        .unwrap_or_default();
                    t.push_str(&format!("bssid=02:00:00:00:00:00\nssid={ssid}\nid={id}\n"));
                    t.push_str(&format!("wpa_state={state}\nip_address=192.168.0.77\n"));
                } else {
                    t.push_str(&format!("wpa_state={state}\n"));
                }
                t
            }
            "SIGNAL_POLL" => "RSSI=-58\nLINKSPEED=72\n".into(),
            "SAVE_CONFIG" => {
                if let Some(p) = &conf {
                    let _ = std::fs::write(p, format!("# saved: {} networks\n", nets.len()));
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o644));
                }
                "OK\n".into()
            }
            _ => "UNKNOWN COMMAND\n".into(),
        };
        send_to(&s, &peer, &reply);
    }
}
