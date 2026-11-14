use crate::h4;
use std::io;
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

pub(super) fn urandom(buf: &mut [u8]) -> io::Result<()> {
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
