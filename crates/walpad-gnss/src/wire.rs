//! The datagram protocol between MediaTek's `mnld` and its GNSS HAL (`gps.default.so`), recovered
//! statically from the stock vendor image (docs/43-gnss.md has the evidence and the confidence of
//! every message).  Both directions are AF_UNIX SOCK_DGRAM in the abstract namespace; the peers
//! use the full 110-byte address length, so the name is padded with NULs to 108 bytes.
//!
//! Layout of a datagram (little endian, no padding): `i32 version (=1)`, `i32 command`, payload.
//! Payload primitives: `i32`, `i64`, `f32`, `f64`, `u8`; `string` = `u8 present`, `i32 len+1`,
//! bytes + NUL (or `u8 0` for NULL); `binary` = `i32 len`, bytes.

/// Abstract name the HAL binds and mnld sends to (we take the HAL's place).
pub const HAL_NAME: &str = "mtk_mnl2hal";
/// Abstract name mnld binds and the HAL sends to.
pub const MNLD_NAME: &str = "mtk_hal2mnl";
/// `sockaddr_un` length passed to bind/sendto by MediaTek's helper (2 + 108).
pub const ADDR_LEN: usize = 110;
pub const VERSION: i32 = 1;

// HAL -> mnld commands (hal2mnl_*)
pub const C_INIT: i32 = 0x65;
pub const C_START: i32 = 0x66;
pub const C_STOP: i32 = 0x67;
pub const C_CLEANUP: i32 = 0x68;
pub const C_INJECT_TIME: i32 = 0x69;
pub const C_INJECT_LOCATION: i32 = 0x6a;
pub const C_DELETE_AIDING: i32 = 0x6b;
pub const C_POSITION_MODE: i32 = 0x6c;

// mnld -> HAL messages (mnl2hal_*)
pub const M_LOCATION: i32 = 0x65;
pub const M_STATUS: i32 = 0x66;
pub const M_SV: i32 = 0x67;
pub const M_NMEA: i32 = 0x68;
pub const M_CAPABILITIES: i32 = 0x69;
pub const M_REQUEST_UTC_TIME: i32 = 0x12d;

/// GpsStatusValue of `M_STATUS`.
pub const ST_NONE: i32 = 0;
pub const ST_SESSION_BEGIN: i32 = 1;
pub const ST_SESSION_END: i32 = 2;
pub const ST_ENGINE_ON: i32 = 3;
pub const ST_ENGINE_OFF: i32 = 4;

pub const LOC_HAS_LAT_LONG: u16 = 0x0001;
pub const LOC_HAS_ALTITUDE: u16 = 0x0002;
pub const LOC_HAS_SPEED: u16 = 0x0004;
pub const LOC_HAS_BEARING: u16 = 0x0008;
pub const LOC_HAS_ACCURACY: u16 = 0x0010;

/// Little-endian datagram writer (MediaTek's `put_*`).
#[derive(Default)]
pub struct Wr(pub Vec<u8>);

impl Wr {
    pub fn new(cmd: i32) -> Wr {
        let mut w = Wr(Vec::with_capacity(64));
        w.int(VERSION);
        w.int(cmd);
        w
    }
    pub fn int(&mut self, v: i32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn long(&mut self, v: i64) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn byte(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    pub fn string(&mut self, s: Option<&str>) -> &mut Self {
        match s {
            None => {
                self.0.push(0);
            }
            Some(s) => {
                self.0.push(1);
                self.int(s.len() as i32 + 1);
                self.0.extend_from_slice(s.as_bytes());
                self.0.push(0);
            }
        }
        self
    }
    pub fn binary(&mut self, b: &[u8]) -> &mut Self {
        self.int(b.len() as i32);
        self.0.extend_from_slice(b);
        self
    }
}

/// Bounds-checked reader (MediaTek's `get_*` silently return 0 past the end; we return None).
pub struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    pub fn new(b: &'a [u8]) -> Rd<'a> {
        Rd { b, p: 0 }
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.p..self.p.checked_add(n)?)?;
        self.p += n;
        Some(s)
    }
    pub fn int(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    pub fn long(&mut self) -> Option<i64> {
        Some(i64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    pub fn byte(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    /// `None` = truncated; `Some(None)` = NULL string.
    pub fn string(&mut self) -> Option<Option<String>> {
        if self.byte()? == 0 {
            return Some(None);
        }
        let n = self.int()?;
        if !(0..=0x8000).contains(&n) {
            return None;
        }
        let raw = self.take(n as usize)?;
        let raw = raw.strip_suffix(&[0]).unwrap_or(raw);
        Some(Some(String::from_utf8_lossy(raw).into_owned()))
    }
    pub fn binary(&mut self) -> Option<&'a [u8]> {
        let n = self.int()?;
        if n < 0 {
            return None;
        }
        self.take(n as usize)
    }
}

/// `GpsLocation_ext` as mnld sends it (88 bytes; the stock HAL keeps only the first 64).
/// Offsets from the HAL's `convertToGnssLocation` and the legacy `GpsLocation`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Location {
    pub flags: u16,
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
    pub speed: f32,
    pub bearing: f32,
    /// Horizontal accuracy in metres when the flags say so (offset 64, else the legacy slot 48).
    pub accuracy: Option<f32>,
    /// UTC milliseconds since the epoch.
    pub time_ms: i64,
}

fn f32_at(b: &[u8], o: usize) -> Option<f32> {
    Some(f32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}
fn f64_at(b: &[u8], o: usize) -> Option<f64> {
    Some(f64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?))
}

impl Location {
    pub fn parse(b: &[u8]) -> Option<Location> {
        if b.len() < 64 {
            return None;
        }
        let flags = u16::from_le_bytes([b[8], b[9]]);
        let acc = [f32_at(b, 64), f32_at(b, 48)]
            .into_iter()
            .flatten()
            .find(|a| a.is_finite() && *a > 0.0 && *a < 1.0e6);
        Some(Location {
            flags,
            lat: f64_at(b, 16)?,
            lon: f64_at(b, 24)?,
            alt: f64_at(b, 32)?,
            speed: f32_at(b, 40)?,
            bearing: f32_at(b, 44)?,
            accuracy: if flags & LOC_HAS_ACCURACY != 0 {
                acc
            } else {
                None
            },
            time_ms: i64::from_le_bytes(b.get(56..64)?.try_into().ok()?),
        })
    }

    /// Latitude/longitude are meaningful (flag set and in range).
    pub fn has_position(&self) -> bool {
        self.flags & LOC_HAS_LAT_LONG != 0
            && self.lat.is_finite()
            && self.lon.is_finite()
            && (-90.0..=90.0).contains(&self.lat)
            && (-180.0..=180.0).contains(&self.lon)
    }

    /// Build the 88-byte blob (tests and the fake mnld).
    pub fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; 88];
        b[0..8].copy_from_slice(&88u64.to_le_bytes());
        b[8..10].copy_from_slice(&self.flags.to_le_bytes());
        b[16..24].copy_from_slice(&self.lat.to_le_bytes());
        b[24..32].copy_from_slice(&self.lon.to_le_bytes());
        b[32..40].copy_from_slice(&self.alt.to_le_bytes());
        b[40..44].copy_from_slice(&self.speed.to_le_bytes());
        b[44..48].copy_from_slice(&self.bearing.to_le_bytes());
        b[56..64].copy_from_slice(&self.time_ms.to_le_bytes());
        b[64..68].copy_from_slice(&self.accuracy.unwrap_or(0.0).to_le_bytes());
        b
    }
}

/// A decoded mnld -> HAL datagram.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    Location(Location),
    Status(i32),
    /// Raw legacy satellite blob (layout unconfirmed: see docs/43-gnss.md); NMEA GSV is used instead.
    Sv(Vec<u8>),
    Nmea {
        time_ms: i64,
        text: String,
    },
    Capabilities(i32),
    RequestUtcTime,
    Other(i32),
}

/// Decode one datagram; `None` for something that is not a protocol message.
pub fn decode(dg: &[u8]) -> Option<Msg> {
    let mut r = Rd::new(dg);
    if r.int()? != VERSION {
        return None;
    }
    let cmd = r.int()?;
    Some(match cmd {
        M_LOCATION => Msg::Location(Location::parse(r.binary()?)?),
        M_STATUS => {
            let b = r.binary()?;
            Msg::Status(i32::from_le_bytes(b.get(..4)?.try_into().ok()?))
        }
        M_SV => Msg::Sv(r.binary()?.to_vec()),
        M_NMEA => {
            let time_ms = r.long()?;
            let text = r.string()?.unwrap_or_default();
            Msg::Nmea {
                time_ms,
                text: text.trim_end_matches(['\r', '\n', '\0']).to_string(),
            }
        }
        M_CAPABILITIES => {
            let b = r.binary()?;
            Msg::Capabilities(i32::from_le_bytes(b.get(..4)?.try_into().ok()?))
        }
        M_REQUEST_UTC_TIME => Msg::RequestUtcTime,
        c => Msg::Other(c),
    })
}

// ---- HAL -> mnld builders ----

pub fn init() -> Vec<u8> {
    Wr::new(C_INIT).0
}
pub fn start() -> Vec<u8> {
    Wr::new(C_START).0
}
pub fn stop() -> Vec<u8> {
    Wr::new(C_STOP).0
}
pub fn cleanup() -> Vec<u8> {
    Wr::new(C_CLEANUP).0
}
/// `GpsPositionMode` standalone (0), periodic (0), `interval_ms`, accuracy 0, time 0, no low power.
pub fn position_mode(interval_ms: i32) -> Vec<u8> {
    let mut w = Wr::new(C_POSITION_MODE);
    w.int(0).int(0).int(interval_ms).int(0).int(0).byte(0);
    w.0
}
/// `gps_inject_time(time, timeReference, uncertainty)`: UTC ms, a monotonic ms stamp, +-ms.
pub fn inject_time(utc_ms: i64, reference_ms: i64, uncertainty_ms: i32) -> Vec<u8> {
    let mut w = Wr::new(C_INJECT_TIME);
    w.long(utc_ms).long(reference_ms).int(uncertainty_ms);
    w.0
}
/// `gps_delete_aiding_data(flags)`: 0xffff = everything (cold start).
pub fn delete_aiding(flags: i32) -> Vec<u8> {
    let mut w = Wr::new(C_DELETE_AIDING);
    w.int(flags);
    w.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc() -> Location {
        Location {
            flags: LOC_HAS_LAT_LONG | LOC_HAS_ALTITUDE | LOC_HAS_ACCURACY,
            lat: 23.75,
            lon: 90.25,
            alt: 12.5,
            speed: 0.5,
            bearing: 90.0,
            accuracy: Some(7.5),
            time_ms: 1_700_000_000_123,
        }
    }

    #[test]
    fn builders_match_the_hal_encoding() {
        // hal2mnl_gps_start: put_int(1), put_int(0x66)
        assert_eq!(start(), [1, 0, 0, 0, 0x66, 0, 0, 0]);
        assert_eq!(init(), [1, 0, 0, 0, 0x65, 0, 0, 0]);
        assert_eq!(stop(), [1, 0, 0, 0, 0x67, 0, 0, 0]);
        let m = position_mode(1000);
        assert_eq!(m.len(), 8 + 5 * 4 + 1);
        assert_eq!(&m[4..8], &[0x6c, 0, 0, 0]);
        assert_eq!(&m[16..20], &1000i32.to_le_bytes());
        let t = inject_time(5, 6, 7);
        assert_eq!(t.len(), 8 + 8 + 8 + 4);
        assert_eq!(&t[8..16], &5i64.to_le_bytes());
    }

    #[test]
    fn string_and_binary_encoding() {
        let mut w = Wr::default();
        w.string(Some("ab")).string(None).binary(&[9, 8]);
        assert_eq!(w.0, [1, 3, 0, 0, 0, b'a', b'b', 0, 0, 2, 0, 0, 0, 9, 8]);
        let mut r = Rd::new(&w.0);
        assert_eq!(r.string(), Some(Some("ab".into())));
        assert_eq!(r.string(), Some(None));
        assert_eq!(r.binary(), Some(&[9u8, 8][..]));
        assert_eq!(r.int(), None);
    }

    #[test]
    fn location_roundtrip_and_flags() {
        let l = loc();
        let mut w = Wr::new(M_LOCATION);
        w.binary(&l.encode());
        match decode(&w.0) {
            Some(Msg::Location(p)) => {
                assert_eq!(p, l);
                assert!(p.has_position());
            }
            other => panic!("{other:?}"),
        }
        // the stock HAL sees only 64 bytes: still parses, accuracy then comes from slot 48
        let mut b = l.encode();
        b.truncate(64);
        b[48..52].copy_from_slice(&3.0f32.to_le_bytes());
        assert_eq!(Location::parse(&b).unwrap().accuracy, Some(3.0));
        // without the lat/long flag there is no position
        let mut n = l;
        n.flags = LOC_HAS_ALTITUDE;
        assert!(!n.has_position());
        // out of range is rejected
        let mut bad = l;
        bad.lat = 123.0;
        assert!(!bad.has_position());
        assert!(Location::parse(&b[..63]).is_none());
    }

    #[test]
    fn status_nmea_and_requests() {
        let mut w = Wr::new(M_STATUS);
        w.binary(&ST_ENGINE_ON.to_le_bytes());
        assert_eq!(decode(&w.0), Some(Msg::Status(3)));

        let mut w = Wr::new(M_NMEA);
        w.long(1234).string(Some("$GPGGA,1*00\r\n")).int(13);
        assert_eq!(
            decode(&w.0),
            Some(Msg::Nmea {
                time_ms: 1234,
                text: "$GPGGA,1*00".into()
            })
        );
        assert_eq!(
            decode(&Wr::new(M_REQUEST_UTC_TIME).0),
            Some(Msg::RequestUtcTime)
        );
        assert_eq!(decode(&Wr::new(0x999).0), Some(Msg::Other(0x999)));
        let mut sv = Wr::new(M_SV);
        sv.binary(&[1, 2, 3]);
        assert_eq!(decode(&sv.0), Some(Msg::Sv(vec![1, 2, 3])));
    }

    #[test]
    fn garbage_is_rejected_not_panicked_on() {
        assert_eq!(decode(&[]), None);
        assert_eq!(decode(&[1, 0, 0]), None);
        // wrong version
        assert_eq!(decode(&[2, 0, 0, 0, 0x65, 0, 0, 0]), None);
        // truncated location blob
        let mut w = Wr::new(M_LOCATION);
        w.binary(&[0; 10]);
        assert_eq!(decode(&w.0), None);
        // binary length larger than the datagram
        let mut w = Wr::new(M_SV);
        w.int(1000);
        assert_eq!(decode(&w.0), None);
        // negative lengths
        let mut w = Wr::new(M_SV);
        w.int(-5);
        assert_eq!(decode(&w.0), None);
    }
}
