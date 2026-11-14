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

mod nvram;
#[cfg(test)]
mod tests;

use crate::controller_command;
use crate::h4;
use nvram::urandom;
pub use nvram::{
    addr_is_valid, format_addr, random_addr, redact_addr, resolve, AddrSource, InitConfig,
    NvRecord, RadioParams, RecordError, RECORD_LEN,
};
use std::io;
use std::os::fd::AsFd;
use std::time::Duration;

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
