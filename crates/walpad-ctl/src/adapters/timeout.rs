use rustix::process::{kill_process_group, Pid, Signal};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::Command;
use std::time::{Duration, Instant};

fn parse_secs(s: &str) -> Option<Duration> {
    let (num, mul) = if let Some(n) = s.strip_suffix('s') {
        (n, 1.0)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 60.0)
    } else if let Some(n) = s.strip_suffix('h') {
        (n, 3600.0)
    } else {
        (s, 1.0)
    };
    let v: f64 = num.parse().ok()?;
    if v < 0.0 || !v.is_finite() {
        return None;
    }
    Some(Duration::from_secs_f64(v * mul))
}

fn parse_sig(s: &str) -> Option<Signal> {
    let u = s.trim().to_ascii_uppercase();
    let u = u.strip_prefix("SIG").unwrap_or(&u);
    match u {
        "KILL" | "9" => Some(Signal::KILL),
        "TERM" | "15" => Some(Signal::TERM),
        "INT" | "2" => Some(Signal::INT),
        "HUP" | "1" => Some(Signal::HUP),
        "QUIT" | "3" => Some(Signal::QUIT),
        "ALRM" | "14" => Some(Signal::ALARM),
        "USR1" | "10" => Some(Signal::USR1),
        "USR2" | "12" => Some(Signal::USR2),
        _ => None,
    }
}

/// Zero-orphan `timeout` implementation (replaces BusyBox `timeout`, whose NOMMU double-fork
/// leaves an un-reaped timer grandchild under PID 1 on every call).
/// Supports: `timeout [-s SIG] [-k KILL_SECS] [-t SECS | SECS] PROG [ARGS...]`
pub fn timeout_cmd(args: &[String]) -> i32 {
    let mut sig = Signal::TERM;
    let mut kill_after = Duration::from_secs(1);
    let mut dur: Option<Duration> = None;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--" {
            i += 1;
            break;
        } else if a == "-s" || a == "--signal" {
            let Some(v) = args.get(i + 1) else { return 125 };
            let Some(parsed) = parse_sig(v) else {
                return 125;
            };
            sig = parsed;
            i += 2;
        } else if let Some(v) = a.strip_prefix("-s").or_else(|| a.strip_prefix("--signal=")) {
            let Some(parsed) = parse_sig(v) else {
                return 125;
            };
            sig = parsed;
            i += 1;
        } else if a == "-k" || a == "--kill-after" {
            let Some(v) = args.get(i + 1) else { return 125 };
            let Some(parsed) = parse_secs(v) else {
                return 125;
            };
            kill_after = parsed;
            i += 2;
        } else if let Some(v) = a
            .strip_prefix("-k")
            .or_else(|| a.strip_prefix("--kill-after="))
        {
            let Some(parsed) = parse_secs(v) else {
                return 125;
            };
            kill_after = parsed;
            i += 1;
        } else if a == "-t" {
            let Some(v) = args.get(i + 1) else { return 125 };
            let Some(parsed) = parse_secs(v) else {
                return 125;
            };
            dur = Some(parsed);
            i += 2;
        } else if a.starts_with('-') && dur.is_none() {
            i += 1;
        } else if dur.is_none() {
            let Some(parsed) = parse_secs(a) else {
                return 125;
            };
            dur = Some(parsed);
            i += 1;
            break;
        } else {
            break;
        }
    }
    let Some(timeout) = dur else { return 125 };
    let Some(prog) = args.get(i) else { return 125 };
    let prog_args = &args[i + 1..];

    let mut cmd = Command::new(prog);
    cmd.args(prog_args);
    cmd.process_group(0);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return 127,
        Err(_) => return 126,
    };
    let start = Instant::now();
    let mut poll_ms = 1u64;
    loop {
        match child.try_wait() {
            Ok(Some(st)) => {
                return st.code().unwrap_or_else(|| 128 + st.signal().unwrap_or(15));
            }
            Ok(None) => {
                let elapsed = start.elapsed();
                if elapsed >= timeout {
                    break;
                }
                let rem = timeout - elapsed;
                std::thread::sleep(Duration::from_millis(poll_ms).min(rem));
                poll_ms = (poll_ms * 2).min(25);
            }
            Err(_) => return 125,
        }
    }
    if let Some(pid) = Pid::from_raw(child.id() as i32) {
        let _ = kill_process_group(pid, sig);
    }
    if sig != Signal::KILL {
        let grace_start = Instant::now();
        while grace_start.elapsed() < kill_after {
            if let Ok(Some(_)) = child.try_wait() {
                return 124;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if let Some(pid) = Pid::from_raw(child.id() as i32) {
            let _ = kill_process_group(pid, Signal::KILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    if sig == Signal::KILL {
        137
    } else {
        124
    }
}
