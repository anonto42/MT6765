use super::parse::{clean_reason, field};
use super::*;

#[test]
fn conf_parsing() {
    assert_eq!(parse_conf(""), (None, false));
    assert_eq!(
        parse_conf("# x\nssid2=57616c506164\nwpa_passphrase=abcdefgh\n"),
        (Some("WalPad".into()), true)
    );
    assert_eq!(parse_conf("wpa_passphrase=\n"), (None, false));
}

#[test]
fn output_fields_and_reasons() {
    let out = "state=failed\nreason=hostapd is not installed\x07\nclients=3\n";
    assert_eq!(field(out, "state"), Some("failed"));
    assert_eq!(field(out, "clients"), Some("3"));
    assert_eq!(field(out, "channel"), None);
    assert_eq!(
        clean_reason(field(out, "reason").unwrap()),
        "hostapd is not installed"
    );
    assert_eq!(clean_reason(&"x".repeat(500)).len(), 120);
}
