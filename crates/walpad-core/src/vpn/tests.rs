//! Unit tests for VPN profile validation, settings, and output parsing.

use super::config::render_settings;
use super::runner::{clean_reason, field};
use super::*;

const KEY: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

fn wg(extra: &str) -> String {
    format!(
        "[Interface]\nPrivateKey = {KEY}\nAddress = 10.8.0.2/32\nDNS = 10.8.0.1\n{extra}\n[Peer]\nPublicKey = {KEY}\nEndpoint = vpn.example.net:51820\nAllowedIPs = 0.0.0.0/0\n"
    )
}

#[test]
fn wireguard_validation() {
    assert_eq!(validate_wireguard(&wg("")), Ok(()));
    assert!(validate_wireguard(&wg("PostUp = touch /x")).is_err());
    assert!(validate_wireguard(&wg("preup=x")).is_err());
    assert!(validate_wireguard("[Interface]\nPrivateKey = short\n").is_err());
    assert!(validate_wireguard("").is_err());
    assert!(validate_wireguard(&wg("").replace("Endpoint", "Endpoin")).is_err());
    assert!(validate_wireguard(&wg("").replace("Address", "Adr")).is_err());
    assert!(validate_wireguard("[Foo]\nx = y\n").is_err());
    // an error never quotes the content
    let bad = wg("").replace(KEY, "notakey-SECRETVALUE");
    assert!(!validate_wireguard(&bad)
        .unwrap_err()
        .contains("SECRETVALUE"));
}

#[test]
fn openvpn_validation() {
    let ok = "client\nremote 198.51.100.7 1194\n<ca>\nup not-a-directive\n</ca>\nauth-user-pass creds.txt\n";
    assert_eq!(validate_openvpn(ok), Ok(()));
    assert!(validate_openvpn("client\n").is_err());
    assert!(validate_openvpn("remote h 1\nup /tmp/x.sh\n").is_err());
    assert!(validate_openvpn("remote h 1\n--script-security 2\n").is_err());
    assert!(validate_openvpn("remote h 1\nplugin foo.so\n").is_err());
    assert!(validate_openvpn("remote h 1\nauth-user-pass\n").is_err());
    assert!(validate_openvpn("remote h 1\nconfig other.ovpn\n").is_err());
    assert!(validate_openvpn("remote h 1\n<ca>\n").is_err());
}

#[test]
fn settings_roundtrip_and_validation() {
    let s = Settings {
        profile: Some("home".into()),
        kill_switch: true,
        dns: vec!["1.1.1.1".into(), "2606:4700::1111".into()],
        proxy: Some("http://10.0.0.2:3128".into()),
    };
    assert_eq!(parse_settings(&render_settings(&s)), s);
    let bad =
        parse_settings("profile=../x\ndns=evil;rm,1.1.1.1\nproxy=http://u:p@h:1\nkill_switch=0\n");
    assert_eq!(
        bad,
        Settings {
            profile: None,
            kill_switch: false,
            dns: vec!["1.1.1.1".into()],
            proxy: None
        }
    );
}

#[test]
fn names_from_paths() {
    assert_eq!(
        name_from_path("/root/Downloads/my vpn (1).conf"),
        "my-vpn--1-"
    );
    assert_eq!(name_from_path("/root/.hidden.ovpn"), "hidden");
    assert!(!name_from_path("/root/").is_empty());
    assert_eq!(
        name_from_path(&format!("/r/{}.conf", "a".repeat(40))).len(),
        24
    );
}

#[test]
fn output_fields() {
    let out = "state=failed\nreason=no handshake\x07\nrx=10\n";
    assert_eq!(field(out, "state"), Some("failed"));
    assert_eq!(field(out, "rx"), Some("10"));
    assert_eq!(field(out, "tx"), None);
    assert_eq!(clean_reason(field(out, "reason").unwrap()), "no handshake");
}
