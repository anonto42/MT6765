use super::*;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

const ADDR: [u8; 6] = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("wpd-btinit-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn record(addr: [u8; 6]) -> Vec<u8> {
    let mut r = vec![0xffu8; 440];
    r[..6].copy_from_slice(&addr);
    r[6..8].copy_from_slice(&[0x60, 0]);
    r[12..18].copy_from_slice(&[7, 0, 0, 0, 5, 7]);
    r[18..25].copy_from_slice(&[3, 4, 0, 0, 0, 0, 0]);
    r[25..27].copy_from_slice(&[0x80, 0]);
    r[27..30].copy_from_slice(&[0xff; 3]);
    r[30..38].fill(0);
    r[51..53].fill(0);
    r
}

/// Controller that reads H:4 commands and answers per `reply(opcode) -> Option<status>`
/// (None = silence).  Returns the received commands when the host side is closed.
fn fake_controller(
    mut peer: UnixStream,
    reply: impl Fn(u16) -> Option<u8> + Send + 'static,
) -> std::thread::JoinHandle<Vec<Vec<u8>>> {
    std::thread::spawn(move || {
        let mut seen = Vec::new();
        loop {
            let mut hdr = [0u8; 4];
            if peer.read_exact(&mut hdr).is_err() {
                return seen;
            }
            assert_eq!(hdr[0], h4::CMD);
            let mut params = vec![0u8; hdr[3] as usize];
            peer.read_exact(&mut params).unwrap();
            let op = u16::from_le_bytes([hdr[1], hdr[2]]);
            seen.push([&hdr[..], &params[..]].concat());
            if let Some(st) = reply(op) {
                let [lo, hi] = op.to_le_bytes();
                // answered in two writes, like the STP ring can split packets
                peer.write_all(&[4, 0x0e, 4, 1]).unwrap();
                std::thread::sleep(Duration::from_millis(5));
                peer.write_all(&[lo, hi, st]).unwrap();
            }
        }
    })
}

fn pair() -> (UnixStream, UnixStream) {
    let (a, b) = UnixStream::pair().unwrap();
    a.set_nonblocking(true).unwrap();
    (a, b)
}

#[test]
fn plan_matches_the_recovered_sequence() {
    let p = build_plan(&ADDR, &RadioParams::default());
    let ops: Vec<u16> = p.iter().map(Step::opcode).collect();
    assert_eq!(ops, [0xfc1a, 0xfc79, 0xfc93, 0xfc7a, 0x0c03]);
    // address is sent least-significant byte first
    assert_eq!(
        p[0].cmd,
        [1, 0x1a, 0xfc, 6, 0x55, 0x44, 0x33, 0x22, 0x11, 0x00]
    );
    assert_eq!(p[1].cmd, [1, 0x79, 0xfc, 8, 7, 0, 0, 0, 5, 7, 0, 0]);
    assert_eq!(p[2].cmd[..4], [1, 0x93, 0xfc, 16]);
    assert!(p[2].cmd[4..].iter().all(|&b| b == 0xff));
    assert_eq!(p[3].cmd, [1, 0x7a, 0xfc, 7, 3, 4, 0, 0, 0, 0, 0]);
    assert_eq!(p[4].cmd, [1, 0x03, 0x0c, 0]);
}

#[test]
fn record_fields_are_taken_from_the_right_offsets() {
    let mut r = record(ADDR);
    r[12..18].copy_from_slice(&[1, 2, 3, 4, 5, 6]);
    r[36..38].copy_from_slice(&[7, 8]);
    r[27..30].copy_from_slice(&[9, 10, 11]);
    r[38..51].copy_from_slice(&[20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32]);
    r[18..25].copy_from_slice(&[41, 42, 43, 44, 45, 46, 47]);
    let rec = NvRecord::parse(&r).unwrap();
    assert_eq!(rec.addr, ADDR);
    assert_eq!(rec.radio.radio, [1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(
        rec.radio.tx_power_offset,
        [9, 10, 11, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32]
    );
    assert_eq!(rec.radio.sleep, [41, 42, 43, 44, 45, 46, 47]);
}

#[test]
fn the_tablets_default_record_equals_the_builtin_defaults() {
    let rec = NvRecord::parse(&record(ADDR)).unwrap();
    assert_eq!(rec.radio, RadioParams::default());
}

#[test]
fn record_validation() {
    assert_eq!(NvRecord::parse(&[0; 10]), Err(RecordError::TooShort(10)));
    assert_eq!(NvRecord::parse(&[0; 440]), Err(RecordError::AllZero));
    assert!(NvRecord::parse(&record(ADDR)).is_ok());
}

#[test]
fn address_validation() {
    assert!(addr_is_valid(&ADDR));
    assert!(!addr_is_valid(&[0; 6]));
    assert!(!addr_is_valid(&[0xff; 6]));
    assert!(!addr_is_valid(&[0x00, 0x00, 0x46, 0x00, 0x00, 0x01]));
    assert!(!addr_is_valid(&[0x01, 0x11, 0x22, 0x33, 0x44, 0x55])); // multicast bit
}

#[test]
fn log_form_of_an_address_hides_all_but_two_bytes() {
    let s = redact_addr(&ADDR);
    assert_eq!(s, "xx:xx:xx:xx:44:55");
    assert!(!s.contains("11") && !s.contains("22") && !s.contains("33"));
}

fn cfg(d: &Path) -> InitConfig {
    InitConfig {
        override_addr: None,
        nvram_path: Some(d.join("nv")),
        addr_store: d.join("var/bt-addr"),
        step_timeout: Duration::from_millis(300),
    }
}

fn fixed_rng(b: u8) -> impl FnMut(&mut [u8]) -> io::Result<()> {
    move |buf: &mut [u8]| {
        buf.fill(b);
        Ok(())
    }
}

#[test]
fn address_comes_from_nvdata_first() {
    let d = tmpdir("nv");
    std::fs::write(d.join("nv"), record(ADDR)).unwrap();
    let (a, s, _) = resolve(&cfg(&d), &mut fixed_rng(0x5a), &mut |_| {}).unwrap();
    assert_eq!((a, s), (ADDR, AddrSource::Nvram));
    assert!(!d.join("var/bt-addr").exists());
}

#[test]
fn override_beats_nvdata() {
    let d = tmpdir("ov");
    std::fs::write(d.join("nv"), record(ADDR)).unwrap();
    let mut c = cfg(&d);
    c.override_addr = Some([0x02, 1, 2, 3, 4, 5]);
    let (a, s, _) = resolve(&c, &mut fixed_rng(0), &mut |_| {}).unwrap();
    assert_eq!((a, s), ([0x02, 1, 2, 3, 4, 5], AddrSource::Override));
}

#[test]
fn placeholder_nvdata_falls_back_to_a_random_address_that_stays_stable() {
    let d = tmpdir("rand");
    std::fs::write(d.join("nv"), record([0x00, 0x00, 0x46, 0x00, 0x00, 0x01])).unwrap();
    let mut notes = Vec::new();
    let (a1, s1, _) = resolve(&cfg(&d), &mut fixed_rng(0x5a), &mut |l| {
        notes.push(l.to_string())
    })
    .unwrap();
    assert_eq!(s1, AddrSource::Generated);
    assert!(addr_is_valid(&a1));
    assert_eq!(a1[0] & 3, 2, "locally administered unicast");
    assert!(notes.iter().any(|n| n.contains("placeholder")));
    let stored = std::fs::read_to_string(d.join("var/bt-addr")).unwrap();
    assert_eq!(stored.trim(), format_addr(&a1));
    // second start: same address, even though the rng would now answer differently
    let (a2, s2, _) = resolve(&cfg(&d), &mut fixed_rng(0x99), &mut |_| {}).unwrap();
    assert_eq!((a2, s2), (a1, AddrSource::Stored));
    // the store is private
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(d.join("var/bt-addr"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o077, 0);
}

#[test]
fn missing_nvdata_uses_defaults_and_generates() {
    let d = tmpdir("nonv");
    let mut notes = Vec::new();
    let (a, s, radio) = resolve(&cfg(&d), &mut fixed_rng(0x33), &mut |l| {
        notes.push(l.to_string())
    })
    .unwrap();
    assert_eq!(s, AddrSource::Generated);
    assert!(addr_is_valid(&a));
    assert_eq!(radio, RadioParams::default());
    assert!(notes.iter().any(|n| n.contains("built-in radio defaults")));
}

#[test]
fn corrupt_stored_address_is_replaced() {
    let d = tmpdir("bad");
    std::fs::create_dir_all(d.join("var")).unwrap();
    std::fs::write(d.join("var/bt-addr"), "not an address\n").unwrap();
    let (_, s, _) = resolve(&cfg(&d), &mut fixed_rng(0x33), &mut |_| {}).unwrap();
    assert_eq!(s, AddrSource::Generated);
}

#[test]
fn full_sequence_against_a_scripted_controller() {
    let (ctrl, peer) = pair();
    let t = fake_controller(peer, |_| Some(0));
    let mut logs = Vec::new();
    let steps = build_plan(&ADDR, &RadioParams::default());
    let r = run(&ctrl, &steps, Duration::from_secs(2), &mut |l| {
        logs.push(l.to_string())
    });
    assert_eq!(
        r,
        InitReport {
            ok: 5,
            ..Default::default()
        }
    );
    drop(ctrl);
    let seen = t.join().unwrap();
    let expect: Vec<Vec<u8>> = steps.iter().map(|s| s.cmd.clone()).collect();
    assert_eq!(seen, expect);
    // no parameter bytes in the log
    let all = logs.join("\n");
    assert!(!all.contains("55") && !all.to_lowercase().contains("ffff"));
}

#[test]
fn unsupported_opcodes_are_skipped_and_errors_do_not_stop_the_rest() {
    let (ctrl, peer) = pair();
    let t = fake_controller(peer, |op| match op {
        0xfc79 => Some(0x01), // unknown command
        0xfc93 => Some(0x12), // invalid parameters
        _ => Some(0),
    });
    let steps = build_plan(&ADDR, &RadioParams::default());
    let r = run(&ctrl, &steps, Duration::from_secs(2), &mut |_| {});
    assert_eq!(
        (r.ok, r.skipped, r.failed, r.timed_out, r.aborted),
        (3, 1, 1, 0, false)
    );
    drop(ctrl);
    assert_eq!(t.join().unwrap().len(), 5);
}

#[test]
fn a_silent_controller_stops_the_sequence_after_one_timeout() {
    let (ctrl, peer) = pair();
    let t = fake_controller(peer, |op| (op != 0xfc79).then_some(0));
    let steps = build_plan(&ADDR, &RadioParams::default());
    let t0 = std::time::Instant::now();
    let r = run(&ctrl, &steps, Duration::from_millis(200), &mut |_| {});
    assert!(t0.elapsed() < Duration::from_secs(2));
    assert_eq!((r.ok, r.timed_out, r.aborted), (1, 1, true));
    drop(ctrl);
    assert_eq!(
        t.join().unwrap().len(),
        2,
        "nothing is sent after the timeout"
    );
}

#[test]
fn unrelated_events_between_replies_are_tolerated() {
    let (ctrl, mut peer) = pair();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let t = std::thread::spawn(move || {
        let mut hdr = [0u8; 4];
        peer.read_exact(&mut hdr).unwrap();
        let mut params = vec![0u8; hdr[3] as usize];
        peer.read_exact(&mut params).unwrap();
        // a vendor event, then the Command Complete
        peer.write_all(&[4, 0xff, 2, 0xaa, 0xbb, 4, 0x0e, 4, 1, 0x1a, 0xfc, 0])
            .unwrap();
    });
    let steps = build_plan(&ADDR, &RadioParams::default());
    let r = run(&ctrl, &steps[..1], Duration::from_secs(2), &mut |_| {});
    assert_eq!((r.ok, r.other_packets), (1, 1));
    t.join().unwrap();
}

#[test]
fn closed_controller_aborts_without_hanging() {
    let (ctrl, peer) = pair();
    drop(peer);
    let steps = build_plan(&ADDR, &RadioParams::default());
    let r = run(&ctrl, &steps, Duration::from_millis(300), &mut |_| {});
    assert!(r.aborted);
    assert_eq!(r.ok, 0);
}
