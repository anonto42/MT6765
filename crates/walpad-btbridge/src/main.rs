//! walpad-btbridge [--stpbt /dev/stpbt] [--vhci /dev/vhci] [--bdaddr AA:BB:CC:DD:EE:FF]
//!                 [--ready /run/walpad/btbridge.hci] [--stats /run/walpad/btbridge.stats]
//!                 [--open-timeout SECS] [--init | --no-init] [--nvram FILE] [--addr-store FILE]
//!
//! Start-up: after /dev/stpbt is open and before hciN exists, the MediaTek vendor init sequence
//! (BD address, radio, TX power offset, sleep timeout, reset; see `init.rs`) is sent unless
//! `--no-init`.  The BD address is `--bdaddr`, else the nvdata record `--nvram`, else a random
//! address kept in `--addr-store`.
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
use walpad_btbridge::init::{self, InitConfig};
use walpad_btbridge::{create_hci, h4, Bridge, Step};

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
    init: InitConfig,
    do_init: bool,
    ready: String,
    stats: String,
    open_timeout: Duration,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        stpbt: "/dev/stpbt".into(),
        vhci: "/dev/vhci".into(),
        bdaddr: None,
        init: InitConfig::default(),
        do_init: true,
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
        match k.as_str() {
            "--init" => {
                a.do_init = true;
                continue;
            }
            "--no-init" => {
                a.do_init = false;
                continue;
            }
            _ => {}
        }
        let v = it.next().ok_or(format!("{k} needs a value"))?;
        match k.as_str() {
            "--stpbt" => a.stpbt = v,
            "--vhci" => a.vhci = v,
            "--bdaddr" => {
                // parse_bdaddr gives the wire order; the init module wants it as printed
                let mut b = h4::parse_bdaddr(&v).ok_or("bad --bdaddr (want AA:BB:CC:DD:EE:FF)")?;
                b.reverse();
                a.bdaddr = Some(b);
            }
            "--nvram" => a.init.nvram_path = if v.is_empty() { None } else { Some(v.into()) },
            "--addr-store" => a.init.addr_store = v.into(),
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

    let mut init_report = None;
    if args.do_init {
        let mut cfg = args.init.clone();
        cfg.override_addr = args.bdaddr;
        match init::initialise(&ctrl, &cfg, &mut |l| log(l)) {
            Ok(r) => init_report = Some(r),
            Err(e) => log(&format!("vendor init skipped: {e}")),
        }
    } else {
        log("vendor init disabled (--no-init)");
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
    if let Some(r) = init_report {
        bridge.stats.init_ok = r.ok.into();
        bridge.stats.init_skipped = r.skipped.into();
        bridge.stats.init_failed = (r.failed + r.timed_out).into();
    }
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
