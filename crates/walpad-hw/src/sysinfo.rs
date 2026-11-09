//! Read-only system facts for the Settings screens (all through the `Root` prefix, all optional:
//! a missing file is "unknown", never a guess).
use crate::{sysfs::read_trim, Root};

pub fn kernel_release(root: &Root) -> Option<String> {
    read_trim(root, "/proc/sys/kernel/osrelease")
}

pub fn alpine_release(root: &Root) -> Option<String> {
    read_trim(root, "/etc/alpine-release")
}

/// (total_kb, available_kb) from /proc/meminfo.
pub fn meminfo(root: &Root) -> Option<(u64, u64)> {
    let t = std::fs::read_to_string(root.p("/proc/meminfo")).ok()?;
    let get = |k: &str| {
        t.lines().find_map(|l| {
            l.strip_prefix(k)?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
        })
    };
    Some((get("MemTotal:")?, get("MemAvailable:")?))
}

/// (total_bytes, free_bytes) of the filesystem holding `/`.
pub fn disk_usage(root: &Root) -> Option<(u64, u64)> {
    let s = rustix::fs::statvfs(root.p("/")).ok()?;
    Some((s.f_blocks * s.f_frsize, s.f_bavail * s.f_frsize))
}

fn le_ip(hex: &str) -> Option<String> {
    let v = u32::from_str_radix(hex, 16).ok()?;
    let b = v.to_le_bytes();
    Some(format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3]))
}

/// Listening IPv4 TCP sockets (address, port) from /proc/net/tcp.
pub fn listening_tcp(root: &Root) -> Vec<(String, u16)> {
    let Ok(t) = std::fs::read_to_string(root.p("/proc/net/tcp")) else {
        return Vec::new();
    };
    t.lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.get(3) != Some(&"0A") {
                return None;
            }
            let (ip, port) = f.get(1)?.split_once(':')?;
            Some((le_ip(ip)?, u16::from_str_radix(port, 16).ok()?))
        })
        .collect()
}

/// Default gateway from /proc/net/route.
pub fn default_gateway(root: &Root) -> Option<String> {
    let t = std::fs::read_to_string(root.p("/proc/net/route")).ok()?;
    t.lines().skip(1).find_map(|l| {
        let f: Vec<&str> = l.split_whitespace().collect();
        (f.get(1) == Some(&"00000000"))
            .then(|| le_ip(f.get(2)?))
            .flatten()
    })
}

pub fn dns_servers(root: &Root) -> Vec<String> {
    std::fs::read_to_string(root.p("/etc/resolv.conf"))
        .map(|t| {
            t.lines()
                .filter_map(|l| Some(l.strip_prefix("nameserver")?.trim().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

pub fn authorized_key_count(root: &Root) -> usize {
    std::fs::read_to_string(root.p("/root/.ssh/authorized_keys"))
        .map(|t| {
            t.lines()
                .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
                .count()
        })
        .unwrap_or(0)
}

/// Mask anything that could be a Wi-Fi secret: `psk=...`, `password ...`, `secret: ...`, long hex keys.
pub fn redact_log_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let lower = line.to_ascii_lowercase();
    let keys = [
        "psk",
        "passphrase",
        "password",
        "secret",
        "wpa_passphrase",
        "token",
    ];
    let mut i = 0;
    let b = line.as_bytes();
    'outer: while i < line.len() {
        for k in keys {
            if lower[i..].starts_with(k) {
                // keep the key, mask the value that follows separators like `=`, `:`, ` `, `"`
                let mut j = i + k.len();
                while j < line.len() && matches!(b[j], b'=' | b':' | b' ' | b'"' | b'\'') {
                    j += 1;
                }
                if j > i + k.len() {
                    out.push_str(&line[i..j]);
                    let mut e = j;
                    while e < line.len()
                        && !b[e].is_ascii_whitespace()
                        && b[e] != b'"'
                        && b[e] != b'\''
                    {
                        e += 1;
                    }
                    if e > j {
                        out.push_str("***");
                    }
                    i = e;
                    continue 'outer;
                }
            }
        }
        let ch = line[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    // 64-hex strings (a derived PSK) anywhere
    let mut res = String::new();
    let mut run = String::new();
    for c in out.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_hexdigit() {
            run.push(c);
        } else {
            if run.len() >= 32 && run.len() <= 128 {
                res.push_str("***");
            } else {
                res.push_str(&run);
            }
            run.clear();
            res.push(c);
        }
    }
    res.pop();
    res
}

/// Last `n` lines of a log file, redacted. Missing file -> empty.
pub fn tail_lines(root: &Root, path: &str, n: usize) -> Vec<String> {
    let Ok(bytes) = std::fs::read(root.p(path)) else {
        return Vec::new();
    };
    let start = bytes.len().saturating_sub(64 * 1024);
    let text = String::from_utf8_lossy(&bytes[start..]);
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..]
        .iter()
        .map(|l| redact_log_line(l))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{put, tmp};

    #[test]
    fn reads_kernel_alpine_memory_and_disk() {
        let r = Root::new(tmp());
        assert_eq!(
            (kernel_release(&r), alpine_release(&r), meminfo(&r)),
            (None, None, None)
        );
        put(&r, "/proc/sys/kernel/osrelease", "4.19.127\n");
        put(&r, "/etc/alpine-release", "3.24.0\n");
        put(
            &r,
            "/proc/meminfo",
            "MemTotal:        3800000 kB\nMemFree: 100 kB\nMemAvailable:    1900000 kB\n",
        );
        assert_eq!(kernel_release(&r).as_deref(), Some("4.19.127"));
        assert_eq!(alpine_release(&r).as_deref(), Some("3.24.0"));
        assert_eq!(meminfo(&r), Some((3_800_000, 1_900_000)));
        let (total, free) = disk_usage(&r).unwrap();
        assert!(total > 0 && free <= total);
    }

    #[test]
    fn network_facts() {
        let r = Root::new(tmp());
        put(&r, "/proc/net/tcp", "  sl  local_address rem_address   st\n   0: 00000000:0016 00000000:0000 0A 0 0\n   1: 0100007F:0050 00000000:0000 01 0 0\n   2: 012AA8C0:0017 00000000:0000 0A 0 0\n");
        assert_eq!(
            listening_tcp(&r),
            vec![
                ("0.0.0.0".to_string(), 22),
                ("192.168.42.1".to_string(), 23)
            ],
            "only LISTEN sockets, little-endian addresses"
        );
        put(&r, "/proc/net/route", "Iface Destination Gateway Flags\nwlan0 00000000 0100A8C0 0003\nwlan0 0000A8C0 00000000 0001\n");
        assert_eq!(default_gateway(&r).as_deref(), Some("192.168.0.1"));
        put(
            &r,
            "/etc/resolv.conf",
            "nameserver 1.1.1.1\nsearch lan\nnameserver 8.8.8.8\n",
        );
        assert_eq!(dns_servers(&r), ["1.1.1.1", "8.8.8.8"]);
        put(
            &r,
            "/root/.ssh/authorized_keys",
            "ssh-ed25519 AAAA x\n\n# comment\nssh-rsa BBBB y\n",
        );
        assert_eq!(authorized_key_count(&r), 2);
        assert_eq!(
            (
                default_gateway(&Root::new(tmp())),
                authorized_key_count(&Root::new(tmp()))
            ),
            (None, 0)
        );
    }

    #[test]
    fn log_lines_never_show_secrets() {
        for (raw, must_not) in [
            ("wpa_cli set_network 0 psk=hunter2hunter2 ok", "hunter2"),
            ("connect password: correct horse battery", "correct"),
            ("SET_NETWORK 1 psk 0dc0d6eb90555ed6419756b9a15ec3e3209b63df707dd508d14581f8982721af", "0dc0d6eb"),
            ("token=abcdef123 done", "abcdef123"),
            ("PSK=\"topsecret\" end", "topsecret"),
        ] {
            let out = redact_log_line(raw);
            assert!(!out.contains(must_not), "{raw:?} -> {out:?}");
            assert!(out.contains("***"));
        }
        assert_eq!(
            redact_log_line("walpad-core: listening on /run/walpad/control.sock"),
            "walpad-core: listening on /run/walpad/control.sock"
        );
        assert_eq!(
            redact_log_line("orientation -> 90 accel x=4000"),
            "orientation -> 90 accel x=4000"
        );
        assert_eq!(
            redact_log_line("café ✓ ünï"),
            "café ✓ ünï",
            "UTF-8 survives"
        );
    }

    #[test]
    fn tail_keeps_the_last_lines_redacted() {
        let r = Root::new(tmp());
        let body: String = (0..300)
            .map(|i| format!("line {i} psk=secret{i}\n"))
            .collect();
        put(&r, "/var/log/x.log", &body);
        let t = tail_lines(&r, "/var/log/x.log", 200);
        assert_eq!(t.len(), 200);
        assert_eq!(t[199], "line 299 psk=***");
        assert!(t.iter().all(|l| !l.contains("secret")));
        assert!(tail_lines(&r, "/var/log/missing.log", 5).is_empty());
    }
}
