use super::*;

fn ev(kind: u16, code: u16, value: i32) -> InputEvent {
    InputEvent {
        sec: 10,
        usec: 500_000,
        kind,
        code,
        value,
    }
}

#[test]
fn roundtrip_and_partial() {
    let a = ev(EV_ABS, ABS_MT_POSITION_X, 400);
    let b = ev(EV_SYN, SYN_REPORT, 0);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&a.to_bytes());
    bytes.extend_from_slice(&b.to_bytes());
    bytes.extend_from_slice(&[1, 2, 3]);
    assert_eq!(parse_events(&bytes), vec![a, b]);
    assert_eq!(a.ms(), 10_500);
    assert_eq!(EVENT_SIZE, 24);
}

#[test]
fn key_bitmap_bits() {
    let mut b = [0u8; KEY_BITMAP_BYTES];
    assert!(!key_bit(&b, KEY_VOLUMEDOWN));
    b[14] = 1 << 2; // 114 = 14 * 8 + 2
    assert!(key_bit(&b, KEY_VOLUMEDOWN));
    assert!(!key_bit(&b, KEY_VOLUMEUP) && !key_bit(&b, KEY_POWER));
    assert!(!key_bit(&b, 1000)); // beyond the buffer
    assert!(!key_bit(&[], 0));
    b[95] = 0x80; // code 767
    assert!(key_bit(&b, 767));
    assert_eq!(EVIOCGKEY_96, (2 << 30) | (96 << 16) | (0x45 << 8) | 0x18);
}

#[test]
fn key_bitmap_from_plain_file() {
    let p = crate::testutil::tmp().join("kpd");
    let mut b = vec![0u8; KEY_BITMAP_BYTES];
    b[14] = 4;
    std::fs::write(&p, &b).unwrap();
    let got = read_key_bitmap(&p).unwrap();
    assert!(key_bit(&got, 114));
    assert!(read_key_bitmap(&p.with_extension("none")).is_err());
}

#[test]
fn reader_reads_file_in_chunks() {
    let dir = crate::testutil::tmp();
    let p = dir.join("event.bin");
    let a = ev(EV_KEY, KEY_POWER, 1);
    let mut data = a.to_bytes().to_vec();
    data.extend_from_slice(&a.to_bytes()[..10]);
    std::fs::write(&p, data).unwrap();
    let mut r = EvdevReader::open(&p).unwrap();
    assert_eq!(r.read_available().unwrap(), vec![a]);
    assert_eq!(wait_readable(&[&r], Duration::from_millis(10)).len(), 1);
}
