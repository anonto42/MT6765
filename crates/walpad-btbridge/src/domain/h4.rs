//! HCI UART (H:4) framing: one packet-type byte followed by the HCI packet.
//!
//! | type | packet  | header after the type byte | payload length            |
//! |------|---------|----------------------------|---------------------------|
//! | 0x01 | command | opcode u16, len u8         | `len`                     |
//! | 0x02 | ACL     | handle u16, len u16 LE     | `len`                     |
//! | 0x03 | SCO     | handle u16, len u8         | `len`                     |
//! | 0x04 | event   | code u8, len u8            | `len`                     |
//! | 0xff | vendor  | (vhci only) opcode, index u16 | fixed: 4 bytes in total |
//!
//! `/dev/stpbt` is a byte stream (a read can return part of a packet or several packets), while
//! `/dev/vhci` takes exactly one packet per write, so the controller->host direction must be
//! re-framed.  On garbage (unknown type, impossible length) the parser drops one byte and resyncs.

pub const CMD: u8 = 0x01;
pub const ACL: u8 = 0x02;
pub const SCO: u8 = 0x03;
pub const EVENT: u8 = 0x04;
pub const VENDOR: u8 = 0xff;

/// `HCI_MAX_FRAME_SIZE` (HCI_MAX_ACL_SIZE 1024 + 4) in 4.19: the largest write `/dev/vhci` accepts,
/// type byte included.
pub const VHCI_MAX_FRAME: usize = 1028;
/// `BT_BUFFER_SIZE` of the MediaTek `stp_chrdev_bt` driver: the largest write `/dev/stpbt` accepts.
pub const STPBT_MAX_WRITE: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    /// First byte is not a packet type allowed in this direction.
    UnknownType(u8),
    /// Header announces a packet bigger than the limit (`total` includes the type byte).
    Oversize { ptype: u8, total: usize },
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::UnknownType(t) => write!(f, "unknown packet type {t:#04x}"),
            FrameError::Oversize { ptype, total } => {
                write!(
                    f,
                    "{} packet of {total} bytes exceeds the limit",
                    type_name(*ptype)
                )
            }
        }
    }
}

pub fn type_name(t: u8) -> &'static str {
    match t {
        CMD => "command",
        ACL => "acl",
        SCO => "sco",
        EVENT => "event",
        VENDOR => "vendor",
        _ => "unknown",
    }
}

/// Length of the packet at the start of `buf` (type byte included).
/// `Ok(None)`: not enough bytes yet to know.  Only types in `allowed` are accepted.
pub fn frame_len(buf: &[u8], allowed: &[u8], max: usize) -> Result<Option<usize>, FrameError> {
    let Some(&t) = buf.first() else {
        return Ok(None);
    };
    if !allowed.contains(&t) {
        return Err(FrameError::UnknownType(t));
    }
    let total = match t {
        CMD | SCO if buf.len() >= 4 => 4 + buf[3] as usize,
        ACL if buf.len() >= 5 => 5 + u16::from_le_bytes([buf[3], buf[4]]) as usize,
        EVENT if buf.len() >= 3 => 3 + buf[2] as usize,
        VENDOR => 4,
        CMD | SCO | ACL | EVENT => return Ok(None),
        other => return Err(FrameError::UnknownType(other)),
    };
    if total > max {
        return Err(FrameError::Oversize { ptype: t, total });
    }
    Ok(Some(total))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Next {
    Packet(Vec<u8>),
    /// One byte was dropped to resync after this error.
    Dropped(FrameError),
}

/// Reassembles H:4 packets from a byte stream.
pub struct Deframer {
    buf: Vec<u8>,
    allowed: &'static [u8],
    max: usize,
    pub dropped_bytes: u64,
}

impl Deframer {
    pub fn new(allowed: &'static [u8], max: usize) -> Self {
        Deframer {
            buf: Vec::with_capacity(4096),
            allowed,
            max,
            dropped_bytes: 0,
        }
    }

    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Bytes held back waiting for the rest of a packet.
    pub fn pending(&self) -> usize {
        self.buf.len()
    }

    pub fn next_packet(&mut self) -> Option<Next> {
        match frame_len(&self.buf, self.allowed, self.max) {
            Ok(Some(n)) if self.buf.len() >= n => Some(Next::Packet(self.buf.drain(..n).collect())),
            Ok(_) => None,
            Err(e) => {
                self.buf.remove(0);
                self.dropped_bytes += 1;
                Some(Next::Dropped(e))
            }
        }
    }
}

/// Controller -> host packet types (what `/dev/stpbt` returns).
pub const FROM_CONTROLLER: &[u8] = &[EVENT, ACL, SCO];
/// Host -> controller packet types (what `/dev/vhci` returns once hci0 exists), plus vhci's own
/// vendor packet that answers the device-creation request.
pub const FROM_HOST: &[u8] = &[CMD, ACL, SCO, VENDOR];

/// Checks that `pkt` (one read from `/dev/vhci`) is exactly one well-formed packet.
pub fn check_single(pkt: &[u8], allowed: &[u8], max: usize) -> Result<(), String> {
    match frame_len(pkt, allowed, max) {
        Ok(Some(n)) if n == pkt.len() => Ok(()),
        Ok(Some(n)) => Err(format!(
            "{} packet: header says {n} bytes, read returned {}",
            type_name(pkt[0]),
            pkt.len()
        )),
        Ok(None) => Err(format!(
            "truncated {} packet of {} bytes",
            type_name(pkt.first().copied().unwrap_or(0)),
            pkt.len()
        )),
        Err(e) => Err(e.to_string()),
    }
}

/// HCI Command Complete (0x0e) for `opcode`: returns the status byte.
pub fn command_complete_status(pkt: &[u8], opcode: u16) -> Option<u8> {
    let [EVENT, 0x0e, len, _ncmd, lo, hi, status, ..] = *pkt else {
        return None;
    };
    (len >= 4 && u16::from_le_bytes([lo, hi]) == opcode).then_some(status)
}

/// MediaTek vendor command that sets the public BD address (opcode 0xFC1A, address little-endian).
pub const MTK_SET_BDADDR: u16 = 0xfc1a;

/// HCI_OP_READ_SYNC_TRAIN_PARAMS.  MediaTek CONNSYS controllers advertise the feature but answer
/// this command with an error, and in 4.19 any failed command in the init sequence aborts
/// `hciconfig hci0 up` (the Halium MT6765 kernel tree patches hci_core.c for exactly this; the
/// running Walton kernel does not have that patch).  The bridge turns the failing reply into a
/// success with neutral parameters; BlueZ only stores them.
pub const READ_SYNC_TRAIN_PARAMS: u16 = 0x0c77;

/// If `pkt` is a failed reply (Command Complete or Command Status with status != 0) to
/// Read Synchronization Train Parameters, return a successful Command Complete to send instead.
pub fn fixup_sync_train(pkt: &[u8]) -> Option<Vec<u8>> {
    let [lo, hi] = READ_SYNC_TRAIN_PARAMS.to_le_bytes();
    let ncmd = match *pkt {
        // Command Complete: 04 0e len ncmd op_lo op_hi status ...
        [EVENT, 0x0e, _, ncmd, l, h, status, ..] if l == lo && h == hi && status != 0 => ncmd,
        // Command Status: 04 0f 04 status ncmd op_lo op_hi
        [EVENT, 0x0f, 4, status, ncmd, l, h] if l == lo && h == hi && status != 0 => ncmd,
        _ => return None,
    };
    // status 0, interval 0x0080 (80 ms), timeout 0x0002ee00 (120 s), service data 0
    let mut v = vec![EVENT, 0x0e, 11, ncmd, lo, hi, 0x00, 0x80, 0x00];
    v.extend_from_slice(&0x0002_ee00u32.to_le_bytes());
    v.push(0);
    Some(v)
}

pub fn parse_bdaddr(s: &str) -> Option<[u8; 6]> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 6 {
        return None;
    }
    let mut out = [0u8; 6];
    for (i, p) in parts.iter().enumerate() {
        if p.len() != 2 {
            return None;
        }
        // written most-significant first, sent least-significant first
        out[5 - i] = u8::from_str_radix(p, 16).ok()?;
    }
    Some(out)
}

pub fn set_bdaddr_cmd(addr_le: [u8; 6]) -> Vec<u8> {
    let [lo, hi] = MTK_SET_BDADDR.to_le_bytes();
    let mut v = vec![CMD, lo, hi, 6];
    v.extend_from_slice(&addr_le);
    v
}

#[cfg(test)]
mod tests;
