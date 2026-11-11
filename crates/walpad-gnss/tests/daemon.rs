//! The daemon against a fake mnld on private abstract socket names (no device, no root).
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use walpad_gnss::daemon::{run, Config};
use walpad_gnss::io::Sock;
use walpad_gnss::nmea::with_checksum;
use walpad_gnss::wire::{self, Location, Wr};

static N: AtomicU32 = AtomicU32::new(0);

struct Rig {
    dir: std::path::PathBuf,
    mnld: Sock,
    hal: String,
    stop: Arc<AtomicBool>,
    th: Option<std::thread::JoinHandle<std::io::Result<()>>>,
}

impl Rig {
    fn new(listen_only: bool) -> Rig {
        let id = format!("{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst));
        let dir = std::env::temp_dir().join(format!("walpad-gnss-test-{id}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let hal = format!("t_mnl2hal_{id}");
        let mnld_name = format!("t_hal2mnl_{id}");
        let mnld = Sock::bind(&mnld_name).unwrap();
        let cfg = Config {
            hal_name: hal.clone(),
            mnld_name,
            state_path: dir.join("gnss.json"),
            nmea_link: Some(dir.join("nmea")),
            sv_dump: Some(dir.join("sv.bin")),
            interval_ms: 1000,
            send_start: !listen_only,
        };
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let th = std::thread::spawn(move || run(&cfg, &s2, &|_| {}));
        // the daemon binds before it sends anything; wait for the state file
        wait(|| dir.join("gnss.json").exists());
        Rig {
            dir,
            mnld,
            hal,
            stop,
            th: Some(th),
        }
    }

    fn feed(&self, dg: &[u8]) {
        self.mnld.send_to(&self.hal, dg).unwrap();
    }

    fn state(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.dir.join("gnss.json")).unwrap_or_default())
            .unwrap_or(Value::Null)
    }

    /// Commands the daemon sent to the fake mnld, until `n` arrived.
    fn commands(&self, n: usize) -> Vec<i32> {
        let mut got = Vec::new();
        let mut buf = vec![0u8; 4096];
        let end = Instant::now() + Duration::from_secs(5);
        while got.len() < n && Instant::now() < end {
            if let Some(len) = self.mnld.recv(&mut buf, 200).unwrap() {
                got.push(i32::from_le_bytes(buf[4..8].try_into().unwrap()));
                assert!(len >= 8);
            }
        }
        got
    }

    fn finish(mut self) -> Vec<i32> {
        self.stop.store(true, Ordering::SeqCst);
        self.th.take().unwrap().join().unwrap().unwrap();
        let mut rest = Vec::new();
        let mut buf = vec![0u8; 4096];
        while let Some(_n) = self.mnld.recv(&mut buf, 50).unwrap() {
            rest.push(i32::from_le_bytes(buf[4..8].try_into().unwrap()));
        }
        rest
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.th.take() {
            let _ = t.join();
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn wait(mut f: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(8);
    while Instant::now() < end {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("condition not reached in time");
}

fn location(lat: f64, lon: f64) -> Vec<u8> {
    let l = Location {
        flags: wire::LOC_HAS_LAT_LONG | wire::LOC_HAS_ALTITUDE | wire::LOC_HAS_ACCURACY,
        lat,
        lon,
        alt: 8.0,
        speed: 0.0,
        bearing: 0.0,
        accuracy: Some(9.0),
        time_ms: 1_700_000_000_000,
    };
    let mut w = Wr::new(wire::M_LOCATION);
    w.binary(&l.encode());
    w.0
}

fn nmea(body: &str) -> Vec<u8> {
    let text = format!("{}\r\n", with_checksum(body));
    let mut w = Wr::new(wire::M_NMEA);
    w.long(1).string(Some(&text)).int(text.len() as i32);
    w.0
}

#[test]
fn session_start_fix_nmea_and_stop() {
    let rig = Rig::new(false);
    // init, position mode, start, then the (sane) clock injected
    let cmds = rig.commands(4);
    assert_eq!(
        cmds,
        [
            wire::C_INIT,
            wire::C_POSITION_MODE,
            wire::C_START,
            wire::C_INJECT_TIME
        ]
    );
    wait(|| rig.state()["state"] == "searching");
    assert!(rig.state()["fix"].is_null());

    let mut st = Wr::new(wire::M_STATUS);
    st.binary(&wire::ST_ENGINE_ON.to_le_bytes());
    rig.feed(&st.0);
    // the NMEA pty: open the symlink the daemon published and read what is forwarded
    let link = rig.dir.join("nmea");
    wait(|| link.exists());
    let f = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags_nonblock()
        .open(&link)
        .unwrap();
    rig.feed(&nmea("GPGSV,1,1,02,03,40,100,31,04,15,270,22"));
    rig.feed(&nmea("GNGSA,A,3,03,,,,,,,,,,,,1.5,1.0,1.1,1"));
    rig.feed(&location(23.75, 90.25));
    wait(|| rig.state()["state"] == "fix");
    let s = rig.state();
    assert_eq!(s["fix"]["lat"], 23.75);
    assert_eq!(s["fix"]["lon"], 90.25);
    assert_eq!(s["fix"]["accuracy"], 9.0);
    assert_eq!(s["engine"], "on");
    wait(|| rig.state()["satellites"]["view"] == 2);
    let s = rig.state();
    assert_eq!(s["satellites"]["used"], 1);
    assert_eq!(s["satellites"]["list"][0]["id"], 3);
    assert_eq!(s["satellites"]["list"][0]["used"], true);
    // 0644 file
    {
        use std::os::unix::fs::PermissionsExt;
        let m = std::fs::metadata(rig.dir.join("gnss.json")).unwrap();
        assert_eq!(m.permissions().mode() & 0o777, 0o644);
    }
    assert_eq!(read_pty_lines(&f, 2).len(), 2);

    // mnld asks for the time: answered with inject_time
    rig.feed(&Wr::new(wire::M_REQUEST_UTC_TIME).0);
    let rest = rig.finish();
    assert!(rest.contains(&wire::C_INJECT_TIME), "{rest:?}");
    assert!(rest.contains(&wire::C_STOP), "{rest:?}");
    assert!(rest.contains(&wire::C_CLEANUP), "{rest:?}");
}

#[test]
fn state_goes_off_on_stop_and_link_is_removed() {
    let rig = Rig::new(true);
    rig.feed(&location(1.0, 2.0));
    wait(|| rig.state()["state"] == "fix");
    let dir = rig.dir.clone();
    // keep the dir: Drop removes it, so look before finishing
    let st = dir.join("gnss.json");
    rig.stop.store(true, Ordering::SeqCst);
    wait(|| {
        std::fs::read_to_string(&st)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .is_some_and(|v| v["state"] == "off")
    });
    wait(|| !Path::new(&dir.join("nmea")).exists());
}

#[test]
fn garbage_datagrams_are_ignored() {
    let rig = Rig::new(true);
    rig.feed(&[]);
    rig.feed(&[1, 0, 0, 0]);
    rig.feed(&[9; 100]);
    rig.feed(&Wr::new(0x555).0);
    rig.feed(&location(5.0, 6.0));
    wait(|| rig.state()["state"] == "fix");
    wait(|| rig.state()["counters"]["other"] == 1);
}

#[test]
fn second_instance_cannot_take_the_socket() {
    let rig = Rig::new(true);
    let r = Sock::bind(&rig.hal);
    assert!(r.is_err());
}

#[test]
fn first_sv_blob_is_saved_for_analysis() {
    let rig = Rig::new(true);
    let mut w = Wr::new(wire::M_SV);
    w.binary(&[7u8; 100]);
    rig.feed(&w.0);
    wait(|| rig.dir.join("sv.bin").exists());
    assert_eq!(std::fs::read(rig.dir.join("sv.bin")).unwrap(), vec![7u8; 100]);
}

trait NonBlock {
    fn custom_flags_nonblock(&mut self) -> &mut Self;
}
impl NonBlock for std::fs::OpenOptions {
    fn custom_flags_nonblock(&mut self) -> &mut Self {
        use std::os::unix::fs::OpenOptionsExt;
        self.custom_flags(0x800) // O_NONBLOCK on Linux
    }
}

fn read_pty_lines(mut f: &std::fs::File, want: usize) -> Vec<String> {
    use std::io::Read;
    let end = Instant::now() + Duration::from_secs(5);
    let mut buf = Vec::new();
    while Instant::now() < end {
        let mut chunk = [0u8; 512];
        if let Ok(n) = f.read(&mut chunk) {
            buf.extend_from_slice(&chunk[..n]);
        }
        let text = String::from_utf8_lossy(&buf).to_string();
        let lines: Vec<String> = text
            .split("\r\n")
            .filter(|l| l.starts_with('$'))
            .map(str::to_string)
            .collect();
        if lines.len() >= want {
            return lines;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Vec::new()
}
