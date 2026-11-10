//! walpad-btbridge [--stpbt /dev/stpbt] [--vhci /dev/vhci] [--bdaddr AA:BB:CC:DD:EE:FF]
//!                 [--ready /run/walpad/btbridge.hci] [--stats /run/walpad/btbridge.stats]
//!                 [--open-timeout SECS]
//!
//! Opening /dev/stpbt powers the Bluetooth function on (the driver allows a single opener);
//! closing it powers it off.  Opening /dev/vhci and asking for a primary controller creates hciN,
//! closing it removes hciN.  Exit codes: 0 stopped by SIGTERM/SIGINT, 2 usage, 3 setup failed,
//! 4 I/O failure while running.
use rustix::fs::{Mode, OFlags};
use std::os::fd::OwnedFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use walpad_btbridge::{controller_command, create_hci, h4, Bridge, Step};

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

fn log(msg: &str) {
    eprintln!("walpad-btbridge: {msg}");
}

struct Args {
    stpbt: String,
    vhci: String,
    bdaddr: Option<[u8; 6]>,
    ready: String,
    stats: String,
    open_timeout: Duration,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        stpbt: "/dev/stpbt".into(),
        vhci: "/dev/vhci".into(),
        bdaddr: None,
        ready: "/run/walpad/btbridge.hci".into(),
        stats: "/run/walpad/btbridge.stats".into(),
        open_timeout: Duration::from_secs(15),
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        if k == "--version" {
            println!("walpad-btbridge {}", env!("CARGO_PKG_VERSION"));
            std::process::exit(0);
        }
        let v = it.next().ok_or(format!("{k} needs a value"))?;
        match k.as_str() {
            "--stpbt" => a.stpbt = v,
            "--vhci" => a.vhci = v,
            "--bdaddr" => {
                a.bdaddr =
                    Some(h4::parse_bdaddr(&v).ok_or("bad --bdaddr (want AA:BB:CC:DD:EE:FF)")?)
            }
            "--ready" => a.ready = v,
            "--stats" => a.stats = v,
            "--open-timeout" => {
                a.open_timeout = Duration::from_secs(v.parse().map_err(|_| "bad --open-timeout")?)
            }
            _ => return Err(format!("unknown option {k}")),
        }
    }
    Ok(a)
}

/// open(2) in a helper thread: opening /dev/stpbt runs the WMT power-on inside the kernel and
/// cannot be interrupted, so the timeout is enforced here (the process then exits).
fn open_timeout(path: &str, timeout: Duration) -> Result<OwnedFd, String> {
    let (tx, rx) = mpsc::channel();
    let p = path.to_string();
    std::thread::spawn(move || {
        let r = rustix::fs::open(
            &p,
            OFlags::RDWR | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        );
        let _ = tx.send(r);
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(fd)) => Ok(fd),
        Ok(Err(e)) => Err(format!("open {path}: {e}")),
        Err(_) => Err(format!("open {path}: no answer in {} s", timeout.as_secs())),
    }
}

fn write_file(path: &str, text: &str) {
    if let Err(e) = std::fs::write(path, text) {
        log(&format!("cannot write {path}: {e}"));
    }
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            log(&e);
            std::process::exit(2);
        }
    };
    // SAFETY: the handler only stores to an atomic, which is async-signal-safe.
    unsafe {
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
    }
    let _ = std::fs::remove_file(&args.ready);

    let t0 = Instant::now();
    log(&format!(
        "opening {} (powers the BT function on)",
        args.stpbt
    ));
    let ctrl = match open_timeout(&args.stpbt, args.open_timeout) {
        Ok(fd) => fd,
        Err(e) => {
            log(&e);
            std::process::exit(3);
        }
    };
    log(&format!(
        "{} open after {} ms",
        args.stpbt,
        t0.elapsed().as_millis()
    ));

    if let Some(addr) = args.bdaddr {
        // MediaTek vendor command; the address itself is not logged
        match controller_command(&ctrl, &h4::set_bdaddr_cmd(addr), Duration::from_secs(3)) {
            Ok((0, _)) => log("BD address set (vendor command 0xfc1a)"),
            Ok((st, _)) => log(&format!(
                "set BD address: controller status {st:#04x}; keeping the default"
            )),
            Err(e) => log(&format!("set BD address failed: {e}; keeping the default")),
        }
    }

    let vhci = match open_timeout(&args.vhci, Duration::from_secs(5)) {
        Ok(fd) => fd,
        Err(e) => {
            log(&format!("{e} (is hci_vhci.ko loaded?)"));
            std::process::exit(3);
        }
    };
    let index = match create_hci(&vhci, Duration::from_secs(5)) {
        Ok(i) => i,
        Err(e) => {
            log(&format!("creating the HCI device failed: {e}"));
            std::process::exit(3);
        }
    };
    log(&format!("hci{index} created; bridging"));
    write_file(&args.ready, &format!("hci{index}\n"));

    let mut bridge = Bridge::new(ctrl, vhci);
    let mut last_stats = Instant::now();
    let code = loop {
        if STOP.load(Ordering::SeqCst) {
            log("signal received, stopping");
            break 0;
        }
        match bridge.step(Duration::from_millis(500), &mut |l| log(l)) {
            Ok(Step::Closed(why)) => {
                log(why);
                break 4;
            }
            Ok(_) => {}
            Err(e) => {
                log(&format!("I/O error: {e}"));
                break 4;
            }
        }
        if last_stats.elapsed() >= Duration::from_secs(10) {
            write_file(&args.stats, &bridge.stats.render());
            last_stats = Instant::now();
        }
    };
    write_file(&args.stats, &bridge.stats.render());
    let s = &bridge.stats;
    log(&format!(
        "exit {code}: to host {} packets / {} bytes, to controller {} packets / {} bytes, dropped {} bytes",
        s.to_host_packets, s.to_host_bytes, s.to_controller_packets, s.to_controller_bytes, s.dropped_bytes
    ));
    let _ = std::fs::remove_file(&args.ready);
    drop(bridge); // closes vhci (hciN goes away) and stpbt (BT function off)
    std::process::exit(code);
}
