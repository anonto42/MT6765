use super::*;

const STAT: &str = "cpu  100 10 50 800 40 5 5 0 0 0\ncpu0 50 5 25 400 20 2 3 0 0 0\nintr 1\n";

#[test]
fn cpu_line_and_delta() {
    let a = parse_cpu(STAT).unwrap();
    assert_eq!(
        a,
        CpuTimes {
            busy: 170,
            total: 1010
        }
    );
    // +100 busy, +100 idle (idle 800 -> 860 and iowait 40 -> 80 = +100)
    let b = parse_cpu("cpu  150 10 100 860 80 5 5 0\n").unwrap();
    assert_eq!(b.busy - a.busy, 100);
    assert_eq!(cpu_pct(a, b), Some(50));
    let all_busy = CpuTimes {
        busy: 270,
        total: 1110,
    };
    assert_eq!(cpu_pct(a, all_busy), Some(100));
    // nothing elapsed or counters reset: unknown, not 0
    assert_eq!(cpu_pct(a, a), None);
    assert_eq!(cpu_pct(b, a), None);
    // four-field kernels, junk and missing lines
    assert!(parse_cpu("cpu  1 2 3 4\n").is_some());
    assert!(parse_cpu("cpu  1 2 3\n").is_none());
    assert!(parse_cpu("cpu0 1 2 3 4\n").is_none());
    assert!(parse_cpu("cpu  a b c d\n").is_none());
    assert!(parse_cpu("").is_none());
}

#[test]
fn meminfo_used_is_total_minus_available() {
    let t = "MemTotal:        2000000 kB\nMemFree: 100 kB\nMemAvailable:    500000 kB\n";
    assert_eq!(parse_meminfo(t), Some((2_000_000, 1_500_000)));
    assert_eq!(parse_meminfo("MemTotal: 10 kB\n"), None);
    assert_eq!(parse_meminfo("MemTotal: 0 kB\nMemAvailable: 0 kB\n"), None);
    assert_eq!(
        parse_meminfo("MemTotal: 10 kB\nMemAvailable: 99 kB\n"),
        Some((10, 0))
    );
}

const NET1: &str = "Inter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n    lo: 9999 1 0 0 0 0 0 0 9999 1 0 0 0 0 0 0\n wlan0: 1000 10 0 0 0 0 0 0 500 5 0 0 0 0 0 0\n ccmni0: 2000 10 0 0 0 0 0 0 100 5 0 0 0 0 0 0\n  eth9: 5 1 0 0 0 0 0 0 5 1 0 0 0 0 0 0\n";

#[test]
fn net_dev_counts_only_the_wanted_interfaces() {
    let v = parse_net_dev(NET1);
    let names: Vec<&str> = v.iter().map(|x| x.0.as_str()).collect();
    assert_eq!(names, ["wlan0", "ccmni0"]);
    assert_eq!(v[0], ("wlan0".into(), 1000, 500));
    assert!(counted_iface("rndis0") && counted_iface("usb0") && counted_iface("ccmni12"));
    assert!(!counted_iface("lo") && !counted_iface("ccmni") && !counted_iface("wlan1"));
    assert!(parse_net_dev("garbage\n wlan0: 1 2\n").is_empty());
}

#[test]
fn net_rate_sums_interfaces_and_skips_resets_and_newcomers() {
    let a = parse_net_dev(NET1);
    let b = vec![
        ("wlan0".to_string(), 5000, 1500),
        ("ccmni0".to_string(), 2000, 100),
        ("usb0".to_string(), 77, 77),
    ];
    // 2 s: down 4000 / 2 s, up 1000 / 2 s; the new usb0 is ignored
    assert_eq!(net_rate(&a, &b, Duration::from_secs(2)), Some((2000, 500)));
    // a counter that went back is skipped, not wrapped into a huge rate
    let reset = vec![
        ("wlan0".to_string(), 10, 10),
        ("ccmni0".to_string(), 2500, 200),
    ];
    assert_eq!(
        net_rate(&a, &reset, Duration::from_secs(1)),
        Some((500, 100))
    );
    assert_eq!(net_rate(&a, &[], Duration::from_secs(1)), None);
    assert_eq!(net_rate(&a, &b, Duration::ZERO), None);
}

fn put(root: &Root, p: &str, c: &str) {
    let f = root.p(p);
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(f, c).unwrap();
}

#[test]
fn samples_only_enabled_items_and_persists_the_switches() {
    let dir = std::env::temp_dir().join(format!("walpad-stats-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let root = Root::new(&dir);
    put(&root, "/proc/stat", STAT);
    put(
        &root,
        "/proc/meminfo",
        "MemTotal: 1000 kB\nMemAvailable: 250 kB\n",
    );
    put(&root, "/proc/net/dev", NET1);
    let mut s = Stats::new(&root);
    let t0 = Instant::now();
    // defaults: CPU and RAM; the first CPU reading has no interval yet
    assert!(s.tick(t0));
    assert_eq!(s.status().ram_used_kb, Some(750));
    assert_eq!(s.status().cpu_pct, None);
    assert!(s.status().rom_total_b.is_none() && s.status().net_down_bps.is_none());
    put(&root, "/proc/stat", "cpu  150 10 100 860 80 5 5 0\n");
    assert!(s.tick(t0 + Duration::from_secs(2)));
    assert_eq!(s.status().cpu_pct, Some(50));
    // not due yet
    assert!(!s.tick(t0 + Duration::from_secs(3)));
    // unreadable files are unavailable, not stale or zero
    std::fs::remove_file(root.p("/proc/meminfo")).unwrap();
    assert!(s.tick(t0 + Duration::from_secs(4)));
    assert_eq!(s.status().ram_used_kb, None);

    let it = s
        .configure(Some(false), Some(false), Some(true), Some(true), t0)
        .unwrap();
    assert_eq!((it.cpu, it.ram, it.rom, it.net), (false, false, true, true));
    assert_eq!(s.status().cpu_pct, None);
    s.tick(t0 + Duration::from_secs(5));
    assert!(s.status().rom_total_b.unwrap() > 0);
    put(
        &root,
        "/proc/net/dev",
        " wlan0: 3000 10 0 0 0 0 0 0 900 5 0 0 0 0 0 0\n ccmni0: 2000 1 0 0 0 0 0 0 100 5 0 0 0 0 0 0\n",
    );
    assert!(s.tick(t0 + Duration::from_secs(7)));
    assert_eq!(s.status().net_down_bps, Some(1000));
    assert_eq!(s.status().net_up_bps, Some(200));
    // survives a restart
    let again = Stats::new(&root);
    assert_eq!(again.status().items, it);
    // everything off: no sampling, no figures
    s.configure(Some(false), Some(false), Some(false), Some(false), t0)
        .unwrap();
    assert!(!s.tick(t0 + Duration::from_secs(60)));
    assert!(s.status().rom_total_b.is_none() && s.status().net_up_bps.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
