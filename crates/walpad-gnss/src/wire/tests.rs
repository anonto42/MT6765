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
