//! MediaTek controller start-up sequence (what Android's `libbluetooth_mtk.so` sends through
//! `BT_VND_OP_FW_CFG` right after `/dev/stpbt` is opened), recovered from the vendor library
//! (docs/30-bluetooth.md, "Recovered vendor init sequence").
//!
//! For chip id 0x6765 (MT6765 + MT6631 A-die) the library selects `bt_init_preload_script_connac`:
//!
//! | # | opcode | name in the vendor log        | parameters                                      |
//! |---|--------|-------------------------------|-------------------------------------------------|
//! | 1 | 0xFC1A | Set_Local_BD_Addr             | BD address, least-significant byte first (6)    |
//! | 2 | 0xFC79 | Set_Radio                     | record[12..18] + record[36..38] (8)             |
//! | 3 | 0xFC93 | Set_TX_Power_Offset           | record[27..30] + record[38..51] (16)            |
//! | 4 | 0xFC7A | Set_Sleep_Timeout             | record[18..25] (7)                              |
//! | 5 | 0x0C03 | RESET                         | none                                            |
//!
//! (`record` is the 438-byte `ap_nvram_btradio_struct` stored in `/APCFG/APRDEB/BT_Addr`.)  Two
//! more entries (0xFCC5 PIP, 0x1804 SSP debug) only run when the Android properties
//! `persist.vendor.bt.pip.enable` / `persist.vendor.bt.sspdebug.enable` are "1"; they are off.
//! Every reply is awaited for 2 s; the vendor code aborts the whole init on any failure, this one
//! skips unsupported opcodes and carries on.
//!
//! Logs carry counters and the last two address bytes only, never parameter bytes.

use crate::controller_command;
use crate::h4;
use std::io;
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Size of `ap_nvram_btradio_struct` (the vendor library rejects any other record size).
pub const RECORD_LEN: usize = 438;
/// The vendor library's built-in default for this chip (also what the tablet's nvdata holds,
/// apart from the BD address): addr, voice, codec, radio, sleep, ... at these offsets.
const DEFAULT_RADIO: [u8; 8] = [0x07, 0x00, 0x00, 0x00, 0x05, 0x07, 0x00, 0x00];
const DEFAULT_TX_POWER_OFFSET: [u8; 16] = [0xff; 16];
const DEFAULT_SLEEP: [u8; 7] = [0x03, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00];
/// Placeholder addresses the vendor library treats as "not provisioned" (then it invents one).
const DEFAULT_ADDRS: [[u8; 6]; 2] = [[0x00, 0x00, 0x46, 0x00, 0x00, 0x01], [0; 6]];

pub const OP_SET_BDADDR: u16 = 0xfc1a;
pub const OP_SET_RADIO: u16 = 0xfc79;
pub const OP_SET_TX_POWER_OFFSET: u16 = 0xfc93;
pub const OP_SET_SLEEP_TIMEOUT: u16 = 0xfc7a;
pub const OP_RESET: u16 = 0x0c03;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub name: &'static str,
    pub cmd: Vec<u8>,
}

impl Step {
    pub fn opcode(&self) -> u16 {
        u16::from_le_bytes([self.cmd[1], self.cmd[2]])
    }
}

fn command(opcode: u16, params: &[u8]) -> Vec<u8> {
    let [lo, hi] = opcode.to_le_bytes();
    let mut v = vec![h4::CMD, lo, hi, params.len() as u8];
    v.extend_from_slice(params);
    v
}

/// The radio-related parameters, from the NVRAM record when there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RadioParams {
    pub radio: [u8; 8],
    pub tx_power_offset: [u8; 16],
    pub sleep: [u8; 7],
}

impl Default for RadioParams {
    fn default() -> Self {
        RadioParams {
            radio: DEFAULT_RADIO,
            tx_power_offset: DEFAULT_TX_POWER_OFFSET,
            sleep: DEFAULT_SLEEP,
        }
    }
}

/// Parsed `BT_Addr` NVRAM record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvRecord {
    /// BD address, most-significant byte first (as printed).
    pub addr: [u8; 6],
    pub radio: RadioParams,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RecordError {
    TooShort(usize),
    AllZero,
}

impl std::fmt::Display for RecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecordError::TooShort(n) => {
                write!(f, "NVRAM record too short ({n} < {RECORD_LEN} bytes)")
            }
            RecordError::AllZero => write!(f, "NVRAM record is all zero"),
        }
    }
}

impl NvRecord {
    /// The file is the record, possibly followed by a 2-byte trailer (440 bytes on the tablet).
    pub fn parse(bytes: &[u8]) -> Result<NvRecord, RecordError> {
        if bytes.len() < RECORD_LEN {
            return Err(RecordError::TooShort(bytes.len()));
        }
        let r = &bytes[..RECORD_LEN];
        if r.iter().all(|&b| b == 0) {
            return Err(RecordError::AllZero);
        }
        let mut radio = [0u8; 8];
        radio[..6].copy_from_slice(&r[12..18]);
        radio[6..].copy_from_slice(&r[36..38]);
        let mut tx = [0u8; 16];
        tx[..3].copy_from_slice(&r[27..30]);
        tx[3..].copy_from_slice(&r[38..51]);
        let mut sleep = [0u8; 7];
        sleep.copy_from_slice(&r[18..25]);
        let mut addr = [0u8; 6];
        addr.copy_from_slice(&r[..6]);
        Ok(NvRecord {
            addr,
            radio: RadioParams {
                radio,
                tx_power_offset: tx,
                sleep,
            },
        })
    }
}

/// Usable public address: unicast, not all zero/ones, not one of the vendor's placeholders.
pub fn addr_is_valid(a: &[u8; 6]) -> bool {
    a[0] & 1 == 0 && *a != [0xff; 6] && !DEFAULT_ADDRS.contains(a)
}

pub fn format_addr(a: &[u8; 6]) -> String {
    a.iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// What may be logged about an address.
pub fn redact_addr(a: &[u8; 6]) -> String {
    format!("xx:xx:xx:xx:{:02X}:{:02X}", a[4], a[5])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddrSource {
    /// `--bdaddr` / `/etc/walpad/bluetooth.conf`
    Override,
    /// `/mnt/vendor/nvdata/APCFG/APRDEB/BT_Addr` (the address the phone firmware used)
    Nvram,
    /// previously generated, kept in `/var/lib/walpad/bt-addr`
    Stored,
    /// newly generated (random, locally administered) and stored
    Generated,
}

impl AddrSource {
    pub fn label(self) -> &'static str {
        match self {
            AddrSource::Override => "config override",
            AddrSource::Nvram => "nvdata",
            AddrSource::Stored => "stored random",
            AddrSource::Generated => "newly generated random",
        }
    }
}

#[derive(Debug, Clone)]
pub struct InitConfig {
    /// `--bdaddr` (display order, most-significant first)
    pub override_addr: Option<[u8; 6]>,
    /// copy of (or the real) `BT_Addr`
    pub nvram_path: Option<PathBuf>,
    pub addr_store: PathBuf,
    pub step_timeout: Duration,
}

impl Default for InitConfig {
    fn default() -> Self {
        InitConfig {
            override_addr: None,
            nvram_path: Some(PathBuf::from("/var/lib/walpad/bt-nvram.bin")),
            addr_store: PathBuf::from("/var/lib/walpad/bt-addr"),
            step_timeout: Duration::from_secs(2),
        }
    }
}

fn parse_addr_text(s: &str) -> Option<[u8; 6]> {
    // parse_bdaddr returns least-significant first
    let mut a = h4::parse_bdaddr(s.trim())?;
    a.reverse();
    Some(a)
}

fn store_addr(path: &Path, a: &[u8; 6]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, format!("{}\n", format_addr(a)))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, path)
}

/// Random locally administered unicast address from `/dev/urandom`.
pub fn random_addr(fill: &mut dyn FnMut(&mut [u8]) -> io::Result<()>) -> io::Result<[u8; 6]> {
    for _ in 0..8 {
        let mut a = [0u8; 6];
        fill(&mut a)?;
        a[0] = (a[0] & 0xfc) | 0x02; // unicast, locally administered
        if addr_is_valid(&a) {
            return Ok(a);
        }
    }
    Err(io::Error::other("random source gave no usable address"))
}

fn urandom(buf: &mut [u8]) -> io::Result<()> {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")?.read_exact(buf)
}

/// Pick the BD address and the radio parameters.  Notes for the log go to `log`.
pub fn resolve(
    cfg: &InitConfig,
    fill: &mut dyn FnMut(&mut [u8]) -> io::Result<()>,
    log: &mut dyn FnMut(&str),
) -> io::Result<([u8; 6], AddrSource, RadioParams)> {
    let mut radio = RadioParams::default();
    let mut nv_addr = None;
    if let Some(p) = &cfg.nvram_path {
        match std::fs::read(p).map(|b| NvRecord::parse(&b)) {
            Ok(Ok(rec)) => {
                radio = rec.radio;
                nv_addr = Some(rec.addr);
            }
            Ok(Err(e)) => log(&format!(
                "{}: {e}; using built-in radio defaults",
                p.display()
            )),
            Err(e) => log(&format!(
                "{}: {e}; using built-in radio defaults",
                p.display()
            )),
        }
    }
    if let Some(a) = cfg.override_addr {
        if addr_is_valid(&a) {
            return Ok((a, AddrSource::Override, radio));
        }
        log("configured BD address is not usable; ignoring it");
    }
    if let Some(a) = nv_addr {
        if addr_is_valid(&a) {
            return Ok((a, AddrSource::Nvram, radio));
        }
        log("nvdata BD address is a placeholder; using a stored random one");
    }
    if let Ok(t) = std::fs::read_to_string(&cfg.addr_store) {
        if let Some(a) = parse_addr_text(&t).filter(addr_is_valid) {
            return Ok((a, AddrSource::Stored, radio));
        }
        log("stored BD address is unusable; generating a new one");
    }
    let a = random_addr(fill)?;
    if let Err(e) = store_addr(&cfg.addr_store, &a) {
        log(&format!(
            "cannot store the generated BD address in {}: {e} (it changes on every start)",
            cfg.addr_store.display()
        ));
    }
    Ok((a, AddrSource::Generated, radio))
}

/// The ordered command list.
pub fn build_plan(addr: &[u8; 6], p: &RadioParams) -> Vec<Step> {
    let mut le = *addr;
    le.reverse();
    vec![
        Step {
            name: "Set_Local_BD_Addr",
            cmd: command(OP_SET_BDADDR, &le),
        },
        Step {
            name: "Set_Radio",
            cmd: command(OP_SET_RADIO, &p.radio),
        },
        Step {
            name: "Set_TX_Power_Offset",
            cmd: command(OP_SET_TX_POWER_OFFSET, &p.tx_power_offset),
        },
        Step {
            name: "Set_Sleep_Timeout",
            cmd: command(OP_SET_SLEEP_TIMEOUT, &p.sleep),
        },
        Step {
            name: "Reset",
            cmd: command(OP_RESET, &[]),
        },
    ]
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct InitReport {
    pub ok: u32,
    /// the controller does not know the opcode (status 0x01/0x11)
    pub skipped: u32,
    /// other non-zero status
    pub failed: u32,
    pub timed_out: u32,
    /// the sequence stopped early (silent controller or I/O error)
    pub aborted: bool,
    /// unrelated packets read while waiting
    pub other_packets: usize,
}

/// Send `steps`, one at a time, each waiting for its Command Complete.
pub fn run<F: AsFd>(
    ctrl: F,
    steps: &[Step],
    timeout: Duration,
    log: &mut dyn FnMut(&str),
) -> InitReport {
    let mut r = InitReport::default();
    for s in steps {
        match controller_command(&ctrl, &s.cmd, timeout) {
            Ok((st, other)) => {
                r.other_packets += other;
                match st {
                    0 => {
                        r.ok += 1;
                        log(&format!("init {} ({:#06x}): ok", s.name, s.opcode()));
                    }
                    0x01 | 0x11 => {
                        r.skipped += 1;
                        log(&format!(
                            "init {} ({:#06x}): not supported by this controller (status {st:#04x}); skipped",
                            s.name,
                            s.opcode()
                        ));
                    }
                    _ => {
                        r.failed += 1;
                        log(&format!(
                            "init {} ({:#06x}): controller status {st:#04x}; continuing",
                            s.name,
                            s.opcode()
                        ));
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                r.timed_out += 1;
                r.aborted = true;
                log(&format!(
                    "init {} ({:#06x}): no reply in {} ms; stopping the sequence",
                    s.name,
                    s.opcode(),
                    timeout.as_millis()
                ));
                break;
            }
            Err(e) => {
                r.aborted = true;
                log(&format!("init {}: {e}; stopping the sequence", s.name));
                break;
            }
        }
    }
    log(&format!(
        "init done: {} ok, {} skipped, {} failed, {} timed out{}",
        r.ok,
        r.skipped,
        r.failed,
        r.timed_out,
        if r.aborted { " (aborted)" } else { "" }
    ));
    r
}

/// Resolve the address and run the whole sequence on `ctrl`.
pub fn initialise<F: AsFd>(
    ctrl: F,
    cfg: &InitConfig,
    log: &mut dyn FnMut(&str),
) -> io::Result<InitReport> {
    let mut notes = Vec::new();
    let resolved = resolve(cfg, &mut urandom, &mut |l| notes.push(l.to_string()));
    for n in &notes {
        log(n);
    }
    let (addr, source, radio) = resolved?;
    log(&format!(
        "BD address {} from {}",
        redact_addr(&addr),
        source.label()
    ));
    Ok(run(
        &ctrl,
        &build_plan(&addr, &radio),
        cfg.step_timeout,
        log,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

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
}
