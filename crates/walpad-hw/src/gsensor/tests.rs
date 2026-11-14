use super::*;
use crate::testutil::{put, tmp};

#[test]
fn parses_signed_hex() {
    assert_eq!(
        parse_gsensor("fffff448 00000100 0000ffff\0junk"),
        Some((-3000, 256, 65535))
    );
    assert_eq!(parse_gsensor("0x10 -0x10 0"), Some((16, -16, 0)));
    assert_eq!(parse_gsensor("zz 1 2"), None);
    assert_eq!(parse_gsensor("1 2"), None);
    assert_eq!(parse_gsensor(""), None);
}

#[test]
fn reads_fake_file_and_missing() {
    let r = Root::new(tmp());
    assert_eq!(read_gsensor(&r), None);
    put(&r, "/dev/gsensor", "00000001 00000002 00000003");
    assert_eq!(read_gsensor(&r), Some((1, 2, 3)));
}
