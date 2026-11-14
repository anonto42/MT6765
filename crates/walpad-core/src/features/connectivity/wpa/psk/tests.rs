use super::*;

#[test]
fn matches_ieee_802_11i_vectors() {
    // wpa_passphrase IEEE password
    assert_eq!(
        hex(&derive("password", b"IEEE")),
        "f42c6fc52df0ebef9ebb4b90b38a5f902e83fe1b135a70e23aed762e9710a12e"
    );
    // wpa_passphrase ThisIsASSID ThisIsAPassword
    assert_eq!(
        hex(&derive("ThisIsAPassword", b"ThisIsASSID")),
        "0dc0d6eb90555ed6419756b9a15ec3e3209b63df707dd508d14581f8982721af"
    );
}

#[test]
fn secret_forms() {
    assert_eq!(
        psk_hex("password", b"IEEE").unwrap(),
        "f42c6fc52df0ebef9ebb4b90b38a5f902e83fe1b135a70e23aed762e9710a12e"
    );
    let raw = "F42C6FC52DF0EBEF9EBB4B90B38A5F902E83FE1B135A70E23AED762E9710A12E";
    assert_eq!(psk_hex(raw, b"x").unwrap(), raw.to_lowercase());
    assert_eq!(psk_hex("12345", b"x"), Err(PskError::BadLength));
    assert_eq!(psk_hex("1234567", b"x"), Err(PskError::BadLength));
    // 13 characters is a valid WPA passphrase (not a WEP-104 key)
    assert!(psk_hex("correct horse", b"Home").is_ok());
    assert_eq!(psk_hex(&"z".repeat(64), b"x"), Err(PskError::BadLength));
}
