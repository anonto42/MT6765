//! walpad-gnss [--state /run/walpad/gnss.json] [--nmea /run/walpad/nmea] [--sv-dump PATH]
//!             [--interval MS] [--listen-only]
//!
//! Binds the abstract socket `mtk_mnl2hal` (the HAL's), starts a positioning session on mnld
//! (`mtk_hal2mnl`) and publishes the state.  Exit codes: 0 stopped by SIGTERM/SIGINT, 2 usage,
//! 3 the socket is taken (another HAL/daemon owns it).  Coordinates are never logged.
use std::sync::atomic::{AtomicBool, Ordering};
use walpad_gnss::daemon::{run, Config};

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

fn log(msg: &str) {
    eprintln!("walpad-gnss: {msg}");
}

fn parse_args() -> Result<Config, String> {
    let mut c = Config::default();
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        match k.as_str() {
            "--version" => {
                println!("walpad-gnss {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--listen-only" => c.send_start = false,
            "--state" | "--nmea" | "--sv-dump" | "--interval" | "--hal-name" | "--mnld-name" => {
                let v = it.next().ok_or(format!("{k} needs a value"))?;
                match k.as_str() {
                    "--state" => c.state_path = v.into(),
                    "--nmea" => c.nmea_link = if v.is_empty() { None } else { Some(v.into()) },
                    "--sv-dump" => c.sv_dump = if v.is_empty() { None } else { Some(v.into()) },
                    "--interval" => {
                        c.interval_ms = v
                            .parse()
                            .ok()
                            .filter(|n| (200..=60_000).contains(n))
                            .ok_or("bad --interval (200-60000 ms)")?
                    }
                    "--hal-name" => c.hal_name = v,
                    _ => c.mnld_name = v,
                }
            }
            _ => return Err(format!("unknown option {k}")),
        }
    }
    Ok(c)
}

fn main() {
    let cfg = match parse_args() {
        Ok(c) => c,
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
    match run(&cfg, &STOP, &log) {
        Ok(()) => {
            log("stopped");
        }
        Err(e) => {
            log(&format!("cannot take the HAL socket: {e}"));
            std::process::exit(3);
        }
    }
}
