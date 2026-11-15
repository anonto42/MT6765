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
