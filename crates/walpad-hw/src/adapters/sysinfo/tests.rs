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
        (
            "SET_NETWORK 1 psk 0dc0d6eb90555ed6419756b9a15ec3e3209b63df707dd508d14581f8982721af",
            "0dc0d6eb",
        ),
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
