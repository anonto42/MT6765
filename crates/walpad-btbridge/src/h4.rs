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
mod tests {
    use super::*;

    fn evt(code: u8, payload: &[u8]) -> Vec<u8> {
        let mut v = vec![EVENT, code, payload.len() as u8];
        v.extend_from_slice(payload);
        v
    }
    fn acl(handle: u16, payload: &[u8]) -> Vec<u8> {
        let mut v = vec![ACL];
        v.extend_from_slice(&handle.to_le_bytes());
        v.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        v.extend_from_slice(payload);
        v
    }
    fn sco(handle: u16, payload: &[u8]) -> Vec<u8> {
        let mut v = vec![SCO];
        v.extend_from_slice(&handle.to_le_bytes());
        v.push(payload.len() as u8);
        v.extend_from_slice(payload);
        v
    }
    fn cmd(opcode: u16, payload: &[u8]) -> Vec<u8> {
        let mut v = vec![CMD];
        v.extend_from_slice(&opcode.to_le_bytes());
        v.push(payload.len() as u8);
        v.extend_from_slice(payload);
        v
    }
    fn drain(d: &mut Deframer) -> Vec<Next> {
        std::iter::from_fn(|| d.next_packet()).collect()
    }
    fn packets(d: &mut Deframer) -> Vec<Vec<u8>> {
        drain(d)
            .into_iter()
            .map(|n| match n {
                Next::Packet(p) => p,
                Next::Dropped(e) => panic!("unexpected drop: {e}"),
            })
            .collect()
    }

    #[test]
    fn lengths_of_all_four_types() {
        let all = &[CMD, ACL, SCO, EVENT];
        assert_eq!(frame_len(&cmd(0x0c03, &[]), all, 4096), Ok(Some(4)));
        assert_eq!(frame_len(&acl(1, &[0; 300]), all, 4096), Ok(Some(305)));
        assert_eq!(frame_len(&sco(1, &[0; 60]), all, 4096), Ok(Some(64)));
        assert_eq!(
            frame_len(&evt(0x0e, &[1, 3, 0x0c, 0]), all, 4096),
            Ok(Some(7))
        );
        assert_eq!(frame_len(&[VENDOR, 0, 0, 0], &[VENDOR], 4096), Ok(Some(4)));
    }

    #[test]
    fn incomplete_headers_wait() {
        let all = &[CMD, ACL, SCO, EVENT];
        assert_eq!(frame_len(&[], all, 4096), Ok(None));
        assert_eq!(frame_len(&[CMD, 3, 0x0c], all, 4096), Ok(None));
        assert_eq!(frame_len(&[ACL, 1, 0, 5], all, 4096), Ok(None));
        assert_eq!(frame_len(&[SCO, 1, 0], all, 4096), Ok(None));
        assert_eq!(frame_len(&[EVENT, 0x0e], all, 4096), Ok(None));
    }

    #[test]
    fn merged_packets_in_one_read() {
        let mut d = Deframer::new(FROM_CONTROLLER, VHCI_MAX_FRAME);
        let (a, b, c) = (
            evt(0x0e, &[1, 3, 0x0c, 0]),
            acl(0x2001, b"hello"),
            sco(0x0006, &[7; 48]),
        );
        let mut stream = a.clone();
        stream.extend(&b);
        stream.extend(&c);
        stream.extend(&a);
        d.push(&stream);
        assert_eq!(packets(&mut d), vec![a.clone(), b, c, a]);
        assert_eq!(d.pending(), 0);
    }

    #[test]
    fn packet_split_byte_by_byte() {
        let mut d = Deframer::new(FROM_CONTROLLER, VHCI_MAX_FRAME);
        let p = acl(0x2001, &[0xab; 1021]);
        for (i, byte) in p.iter().enumerate() {
            d.push(&[*byte]);
            let got = packets(&mut d);
            if i + 1 < p.len() {
                assert!(got.is_empty(), "emitted early at byte {i}");
            } else {
                assert_eq!(got, vec![p.clone()]);
            }
        }
    }

    #[test]
    fn split_across_header_and_merged_tail() {
        let mut d = Deframer::new(FROM_CONTROLLER, VHCI_MAX_FRAME);
        let (a, b) = (evt(0x3e, &[0x02; 40]), acl(0x2002, &[1, 2, 3]));
        let mut stream = a.clone();
        stream.extend(&b);
        d.push(&stream[..2]); // type + event code only
        assert!(packets(&mut d).is_empty());
        d.push(&stream[2..a.len() + 3]); // rest of a + start of b's header
        assert_eq!(packets(&mut d), vec![a.clone()]);
        d.push(&stream[a.len() + 3..]);
        assert_eq!(packets(&mut d), vec![b]);
    }

    #[test]
    fn zero_length_payloads() {
        let mut d = Deframer::new(FROM_CONTROLLER, VHCI_MAX_FRAME);
        let (a, b, c) = (evt(0x10, &[]), acl(1, &[]), sco(1, &[]));
        d.push(&[a.clone(), b.clone(), c.clone()].concat());
        assert_eq!(packets(&mut d), vec![a, b, c]);
    }

    #[test]
    fn oversize_acl_is_dropped_and_resyncs() {
        let mut d = Deframer::new(FROM_CONTROLLER, VHCI_MAX_FRAME);
        // ACL header announcing 2000 bytes (> 1028 total): not forwardable to vhci
        let bad = [ACL, 0x01, 0x20, 0xd0, 0x07];
        let good = evt(0x0e, &[1, 3, 0x0c, 0]);
        d.push(&bad);
        d.push(&good);
        let out = drain(&mut d);
        assert_eq!(
            out[0],
            Next::Dropped(FrameError::Oversize {
                ptype: ACL,
                total: 2005
            })
        );
        assert_eq!(out.last(), Some(&Next::Packet(good)));
        assert!(d.dropped_bytes >= 1);
    }

    #[test]
    fn exactly_max_is_accepted() {
        let mut d = Deframer::new(FROM_CONTROLLER, VHCI_MAX_FRAME);
        let p = acl(1, &[0; VHCI_MAX_FRAME - 5]);
        d.push(&p);
        assert_eq!(packets(&mut d), vec![p]);
    }

    #[test]
    fn unknown_type_resyncs() {
        let mut d = Deframer::new(FROM_CONTROLLER, VHCI_MAX_FRAME);
        let good = evt(0x0f, &[0, 1, 5, 4]);
        d.push(&[0x00, 0x99]);
        d.push(&good);
        let out = drain(&mut d);
        assert_eq!(out[0], Next::Dropped(FrameError::UnknownType(0x00)));
        assert_eq!(out[1], Next::Dropped(FrameError::UnknownType(0x99)));
        assert_eq!(out[2], Next::Packet(good));
        assert_eq!(d.dropped_bytes, 2);
    }

    #[test]
    fn command_is_not_a_controller_packet() {
        let mut d = Deframer::new(FROM_CONTROLLER, VHCI_MAX_FRAME);
        d.push(&[CMD]);
        assert_eq!(
            d.next_packet(),
            Some(Next::Dropped(FrameError::UnknownType(CMD)))
        );
    }

    #[test]
    fn single_packet_check_for_vhci_reads() {
        assert!(check_single(&cmd(0x0c03, &[]), FROM_HOST, STPBT_MAX_WRITE).is_ok());
        assert!(check_single(&acl(1, &[0; 1021]), FROM_HOST, STPBT_MAX_WRITE).is_ok());
        assert!(check_single(&[VENDOR, 0, 0, 0], FROM_HOST, STPBT_MAX_WRITE).is_ok());
        assert!(check_single(&cmd(0x0c03, &[1])[..4], FROM_HOST, STPBT_MAX_WRITE).is_err());
        assert!(check_single(
            &[cmd(0x0c03, &[]), cmd(0x0c03, &[])].concat(),
            FROM_HOST,
            STPBT_MAX_WRITE
        )
        .is_err());
        assert!(check_single(&evt(0x0e, &[]), FROM_HOST, STPBT_MAX_WRITE).is_err());
        assert!(check_single(&acl(1, &[0; 3000]), FROM_HOST, STPBT_MAX_WRITE).is_err());
    }

    #[test]
    fn sync_train_failure_is_turned_into_success() {
        let failed_cc = evt(0x0e, &[1, 0x77, 0x0c, 0x01]);
        let fixed = fixup_sync_train(&failed_cc).unwrap();
        assert_eq!(
            frame_len(&fixed, FROM_CONTROLLER, VHCI_MAX_FRAME),
            Ok(Some(fixed.len()))
        );
        assert_eq!(
            command_complete_status(&fixed, READ_SYNC_TRAIN_PARAMS),
            Some(0)
        );
        assert_eq!(fixed.len(), 3 + 11);
        let failed_cs = evt(0x0f, &[0x11, 2, 0x77, 0x0c]);
        assert_eq!(fixup_sync_train(&failed_cs).unwrap()[3], 2); // keeps the controller's ncmd
                                                                 // a successful reply and other commands pass through untouched
        assert_eq!(
            fixup_sync_train(&evt(0x0e, &[1, 0x77, 0x0c, 0, 0x80, 0, 0, 0xee, 2, 0, 0])),
            None
        );
        assert_eq!(fixup_sync_train(&evt(0x0e, &[1, 0x03, 0x0c, 1])), None);
        assert_eq!(fixup_sync_train(&evt(0x0f, &[0x01, 1, 0x03, 0x0c])), None);
    }

    #[test]
    fn bdaddr_and_command_complete() {
        assert_eq!(
            parse_bdaddr("00:11:22:33:44:5A"),
            Some([0x5a, 0x44, 0x33, 0x22, 0x11, 0x00])
        );
        assert_eq!(parse_bdaddr("00:11:22:33:44"), None);
        assert_eq!(parse_bdaddr("00:11:22:33:44:zz"), None);
        let c = set_bdaddr_cmd([1, 2, 3, 4, 5, 6]);
        assert_eq!(c, vec![CMD, 0x1a, 0xfc, 6, 1, 2, 3, 4, 5, 6]);
        assert_eq!(
            command_complete_status(&evt(0x0e, &[1, 0x1a, 0xfc, 0]), MTK_SET_BDADDR),
            Some(0)
        );
        assert_eq!(
            command_complete_status(&evt(0x0e, &[1, 0x03, 0x0c, 0]), MTK_SET_BDADDR),
            None
        );
        assert_eq!(
            command_complete_status(&evt(0x0f, &[0, 1, 0x1a, 0xfc]), MTK_SET_BDADDR),
            None
        );
    }
}
