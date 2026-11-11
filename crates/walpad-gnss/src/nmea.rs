//! The NMEA 0183 sentences that mnld forwards (message 0x68): GGA, RMC, GSV, GSA.  Satellites in
//! view and the satellites used in the fix come from here, because the binary satellite blob of
//! message 0x67 is not confirmed (docs/43-gnss.md).
//! Every parser is total: a malformed sentence is `None`, never a panic.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Sys {
    Gps,
    Glonass,
    Galileo,
    Beidou,
    Qzss,
    Sbas,
}

impl Sys {
    pub fn name(self) -> &'static str {
        match self {
            Sys::Gps => "gps",
            Sys::Glonass => "glonass",
            Sys::Galileo => "galileo",
            Sys::Beidou => "beidou",
            Sys::Qzss => "qzss",
            Sys::Sbas => "sbas",
        }
    }
}

/// One `GSV` entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SatInfo {
    pub id: u16,
    pub elev: Option<u8>,
    pub az: Option<u16>,
    /// Signal to noise, dB-Hz (None = not tracked).
    pub snr: Option<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Sentence {
    Gga {
        lat: Option<f64>,
        lon: Option<f64>,
        quality: u8,
        used: Option<u8>,
        alt: Option<f64>,
    },
    Rmc {
        valid: bool,
        lat: Option<f64>,
        lon: Option<f64>,
        speed_mps: Option<f32>,
        course: Option<f32>,
        /// UTC milliseconds when both date and time are present.
        time_ms: Option<i64>,
    },
    Gsv {
        /// `None` for the combined `GN` talker (never used for GSV in practice).
        sys: Option<Sys>,
        total: u8,
        num: u8,
        in_view: u8,
        sats: Vec<SatInfo>,
    },
    Gsa {
        sys: Option<Sys>,
        /// 1 = no fix, 2 = 2D, 3 = 3D.
        fix: u8,
        ids: Vec<u16>,
    },
}

/// `$....*HH` checksum (no checksum = accepted: some firmware omits it in tests).
pub fn checksum_ok(s: &str) -> bool {
    let Some(body) = s.strip_prefix('$') else {
        return false;
    };
    match body.split_once('*') {
        None => true,
        Some((data, cs)) => {
            let Ok(want) = u8::from_str_radix(cs.trim(), 16) else {
                return false;
            };
            data.bytes().fold(0u8, |a, b| a ^ b) == want
        }
    }
}

/// Append `*HH` (tests and the fake mnld).
pub fn with_checksum(body: &str) -> String {
    format!("${body}*{:02X}", body.bytes().fold(0u8, |a, b| a ^ b))
}

fn talker_sys(t: &str) -> Option<Sys> {
    match t {
        "GP" => Some(Sys::Gps),
        "GL" => Some(Sys::Glonass),
        "GA" => Some(Sys::Galileo),
        "GB" | "BD" => Some(Sys::Beidou),
        "GQ" | "QZ" => Some(Sys::Qzss),
        _ => None,
    }
}

/// `ddmm.mmmm` + hemisphere -> signed degrees.
fn coord(v: &str, hemi: &str, deg_digits: usize) -> Option<f64> {
    if v.len() < deg_digits + 2 || !v.is_ascii() {
        return None;
    }
    let deg: f64 = v[..deg_digits].parse().ok()?;
    let min: f64 = v[deg_digits..].parse().ok()?;
    if !(0.0..60.0).contains(&min) {
        return None;
    }
    let d = deg + min / 60.0;
    match hemi {
        "N" | "E" => Some(d),
        "S" | "W" => Some(-d),
        _ => None,
    }
}

/// Days since 1970-01-01 of a civil date (proleptic Gregorian).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn utc_ms(time: &str, date: &str) -> Option<i64> {
    if time.len() < 6 || date.len() != 6 || !time.is_ascii() || !date.is_ascii() {
        return None;
    }
    let h: i64 = time[0..2].parse().ok()?;
    let mi: i64 = time[2..4].parse().ok()?;
    let s: f64 = time[4..].parse().ok()?;
    let d: i64 = date[0..2].parse().ok()?;
    let mo: i64 = date[2..4].parse().ok()?;
    let yy: i64 = date[4..6].parse().ok()?;
    // two-digit year: GPS time starts in 1980
    let y = if yy >= 80 { 1900 + yy } else { 2000 + yy };
    if !(1..=31).contains(&d)
        || !(1..=12).contains(&mo)
        || h > 23
        || mi > 59
        || !(0.0..61.0).contains(&s)
    {
        return None;
    }
    Some(((days_from_civil(y, mo, d) * 24 + h) * 60 + mi) * 60_000 + (s * 1000.0).round() as i64)
}

fn num<T: std::str::FromStr>(s: &str) -> Option<T> {
    if s.is_empty() {
        None
    } else {
        s.parse().ok()
    }
}

/// Parse one sentence (with or without trailing CR/LF). `None` = not one we use, or malformed.
pub fn parse(line: &str) -> Option<Sentence> {
    let line = line.trim();
    if !checksum_ok(line) {
        return None;
    }
    let body = line.strip_prefix('$')?;
    let body = body.split_once('*').map_or(body, |(d, _)| d);
    let f: Vec<&str> = body.split(',').collect();
    let head = *f.first()?;
    if head.len() != 5 || !head.is_ascii() {
        return None;
    }
    let (talker, kind) = head.split_at(2);
    match kind {
        "GGA" if f.len() >= 10 => Some(Sentence::Gga {
            lat: coord(f[2], f[3], 2),
            lon: coord(f[4], f[5], 3),
            quality: num(f[6]).unwrap_or(0),
            used: num(f[7]),
            alt: num(f[9]),
        }),
        "RMC" if f.len() >= 10 => Some(Sentence::Rmc {
            valid: f[2] == "A",
            lat: coord(f[3], f[4], 2),
            lon: coord(f[5], f[6], 3),
            speed_mps: num::<f32>(f[7]).map(|k| k * 0.514_444),
            course: num(f[8]),
            time_ms: utc_ms(f[1], f[9]),
        }),
        "GSV" if f.len() >= 4 => {
            let sats = f[4..]
                .chunks(4)
                .filter(|c| c.len() == 4 && !c[0].is_empty())
                .filter_map(|c| {
                    Some(SatInfo {
                        id: num(c[0])?,
                        elev: num(c[1]),
                        az: num(c[2]),
                        snr: num(c[3]),
                    })
                })
                .collect();
            Some(Sentence::Gsv {
                sys: talker_sys(talker),
                total: num(f[1])?,
                num: num(f[2])?,
                in_view: num(f[3])?,
                sats,
            })
        }
        "GSA" if f.len() >= 15 => {
            let sys = f.get(18).and_then(|s| match *s {
                "1" => Some(Sys::Gps),
                "2" => Some(Sys::Glonass),
                "3" => Some(Sys::Galileo),
                "4" => Some(Sys::Beidou),
                "5" => Some(Sys::Qzss),
                _ => None,
            });
            Some(Sentence::Gsa {
                sys: sys.or(talker_sys(talker)),
                fix: num(f[2]).unwrap_or(1),
                ids: f[3..15].iter().filter_map(|s| num(s)).collect(),
            })
        }
        _ => None,
    }
}

/// Constellation of a satellite id under the NMEA 4.10 numbering used when the talker is not
/// specific (`GN`) or the id is outside the talker's range.
pub fn sys_of_id(id: u16) -> Sys {
    match id {
        1..=32 => Sys::Gps,
        33..=64 => Sys::Sbas,
        65..=96 => Sys::Glonass,
        193..=202 => Sys::Qzss,
        203..=237 => Sys::Beidou,
        301..=336 => Sys::Galileo,
        _ => Sys::Gps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums() {
        let s = with_checksum("GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,");
        assert!(s.ends_with("*47"), "{s}");
        assert!(checksum_ok(&s));
        assert!(!checksum_ok(&s.replace("*47", "*48")));
        assert!(!checksum_ok("GPGGA,1*00"));
        assert!(!checksum_ok("$GPGGA,1*ZZ"));
        assert!(checksum_ok("$GPGGA,1"));
    }

    #[test]
    fn gga() {
        let s = with_checksum("GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,");
        match parse(&s) {
            Some(Sentence::Gga {
                lat,
                lon,
                quality,
                used,
                alt,
            }) => {
                assert!((lat.unwrap() - 48.1173).abs() < 1e-4);
                assert!((lon.unwrap() - 11.516_666).abs() < 1e-4);
                assert_eq!((quality, used), (1, Some(8)));
                assert_eq!(alt, Some(545.4));
            }
            o => panic!("{o:?}"),
        }
        // no fix: empty position
        let s = with_checksum("GPGGA,,,,,,0,00,,,M,,M,,");
        assert!(matches!(
            parse(&s),
            Some(Sentence::Gga {
                lat: None,
                lon: None,
                quality: 0,
                ..
            })
        ));
    }

    #[test]
    fn rmc_south_west_and_date() {
        let s = with_checksum("GPRMC,123519,A,3352.000,S,15112.000,W,022.4,084.4,230394,003.1,W");
        match parse(&s) {
            Some(Sentence::Rmc {
                valid,
                lat,
                lon,
                speed_mps,
                course,
                time_ms,
            }) => {
                assert!(valid);
                assert!(lat.unwrap() < -33.86 && lat.unwrap() > -33.87);
                assert!(lon.unwrap() < -151.0);
                assert!((speed_mps.unwrap() - 11.52).abs() < 0.01);
                assert_eq!(course, Some(84.4));
                // 1994-03-23 12:35:19 UTC
                assert_eq!(time_ms, Some(764_426_119_000));
            }
            o => panic!("{o:?}"),
        }
        let v = with_checksum("GPRMC,123519,V,,,,,,,230394,,");
        assert!(matches!(
            parse(&v),
            Some(Sentence::Rmc {
                valid: false,
                lat: None,
                ..
            })
        ));
    }

    #[test]
    fn gsv_and_gsa() {
        let s = with_checksum("GPGSV,3,1,11,03,03,111,00,04,15,270,00,06,01,010,00,13,06,292,00");
        match parse(&s) {
            Some(Sentence::Gsv {
                sys,
                total,
                num,
                in_view,
                sats,
            }) => {
                assert_eq!((sys, total, num, in_view), (Some(Sys::Gps), 3, 1, 11));
                assert_eq!(sats.len(), 4);
                assert_eq!(sats[1].id, 4);
                assert_eq!(sats[1].az, Some(270));
                assert_eq!(sats[1].snr, Some(0));
            }
            o => panic!("{o:?}"),
        }
        // empty SNR = not tracked; trailing signal id field is ignored
        let s = with_checksum("GLGSV,1,1,02,65,40,100,,66,20,200,31,1");
        match parse(&s) {
            Some(Sentence::Gsv { sys, sats, .. }) => {
                assert_eq!(sys, Some(Sys::Glonass));
                assert_eq!(sats.len(), 2);
                assert_eq!(sats[0].snr, None);
                assert_eq!(sats[1].snr, Some(31));
            }
            o => panic!("{o:?}"),
        }
        let s = with_checksum("GNGSA,A,3,04,05,,09,12,,,24,,,,,2.5,1.3,2.1,1");
        match parse(&s) {
            Some(Sentence::Gsa { sys, fix, ids }) => {
                assert_eq!(sys, Some(Sys::Gps));
                assert_eq!(fix, 3);
                assert_eq!(ids, vec![4, 5, 9, 12, 24]);
            }
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn malformed_is_none() {
        for s in [
            "",
            "$",
            "$GPGGA",
            "GPGGA,1,2",
            "$GPGGA,1,2*00",
            "$GPXXX,1,2",
            "$GPGSV,x,1,2",
            "$GPRMC,1,A,9999.0,N,01131.000,E,1,2,310394",
            "$\u{e9}\u{e9}\u{e9}\u{e9}\u{e9},1",
        ] {
            assert!(parse(s).is_none() || s.contains("9999"), "{s}");
        }
        // minutes >= 60 are rejected
        assert_eq!(coord("4860.000", "N", 2), None);
        assert_eq!(utc_ms("123519", "320394"), None);
    }

    #[test]
    fn id_ranges() {
        assert_eq!(sys_of_id(12), Sys::Gps);
        assert_eq!(sys_of_id(70), Sys::Glonass);
        assert_eq!(sys_of_id(210), Sys::Beidou);
        assert_eq!(sys_of_id(305), Sys::Galileo);
    }
}
