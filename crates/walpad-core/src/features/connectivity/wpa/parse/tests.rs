use super::*;

#[test]
fn escapes_and_non_utf8() {
    assert_eq!(decode_escapes("Home"), b"Home");
    assert_eq!(decode_escapes(r"caf\xc3\xa9"), "café".as_bytes());
    assert_eq!(decode_escapes(r"a\\b\"), b"a\\b\\");
    assert_eq!(decode_escapes(r#"say \"hi\""#), b"say \"hi\"");
    assert_eq!(
        decode_escapes(r"\xff\xfe"),
        vec![0xff, 0xfe],
        "non-UTF-8 stays as raw bytes"
    );
    assert_eq!(ssid_display(&[0xff, b'A']), "\u{fffd}A");
    assert_eq!(decode_escapes(r"bad\xZZ"), b"bad\\xZZ");
    assert_eq!(hex(b"Home"), "486f6d65");
    assert_eq!(unhex("486f6d65"), Some(b"Home".to_vec()));
    assert_eq!(unhex("4"), None);
    assert_eq!(unhex("zz"), None);
}

#[test]
fn scan_merge_sort_and_security() {
    let t = "bssid / frequency / signal level / flags / ssid\n\
             aa:aa\t2412\t-70\t[WPA2-PSK-CCMP][ESS]\tHome\n\
             bb:bb\t5180\t-52\t[WPA2-PSK-CCMP][ESS]\tHome\n\
             cc:cc\t2437\t-60\t[ESS]\tCaf\\xc3\\xa9 Open\n\
             dd:dd\t2462\t-80\t[WEP][ESS]\tOldRouter\n\
             ee:ee\t2462\t-40\t[WPA-PSK-TKIP][ESS]\t\n\
             ff:ff\t2462\t-45\t[RSN-SAE-CCMP][ESS]\tSae\n\
             junk line\n";
    let r = parse_scan_results(t);
    let names: Vec<String> = r.iter().map(|e| ssid_display(&e.ssid)).collect();
    assert_eq!(
        names,
        vec!["Sae", "Home", "Café Open", "OldRouter"],
        "sorted by signal, hidden dropped, duplicates merged"
    );
    assert_eq!(r[1].signal_dbm, -52, "strongest AP of Home kept");
    assert_eq!(r[1].freq_mhz, 5180);
    let sec: Vec<&str> = r.iter().map(|e| e.security.as_str()).collect();
    assert_eq!(sec, vec!["wpa", "wpa", "open", "wep"]);
}

#[test]
fn list_status_signal_events() {
    let l = "network id / ssid / bssid / flags\n0\tHome\tany\t[CURRENT]\n3\tCaf\\xc3\\xa9\tany\t[DISABLED]\n";
    let v = parse_list_networks(l);
    assert_eq!((v[0].id, v[0].ssid.clone()), (0, b"Home".to_vec()));
    assert_eq!(v[1].ssid, "Café".as_bytes());
    let s =
        parse_status("bssid=aa\nssid=Home\nid=0\nwpa_state=COMPLETED\nip_address=192.168.0.5\n");
    assert_eq!(
        (s.state.as_deref(), s.ssid, s.ip.as_deref(), s.id),
        (
            Some("COMPLETED"),
            Some(b"Home".to_vec()),
            Some("192.168.0.5"),
            Some(0)
        )
    );
    assert_eq!(parse_signal_poll("RSSI=-58\nLINKSPEED=72\n"), Some(-58));
    assert_eq!(parse_signal_poll("FAIL"), None);
    assert_eq!(
        parse_event("<3>CTRL-EVENT-SCAN-RESULTS "),
        Event::ScanResults
    );
    assert_eq!(
        parse_event("<3>CTRL-EVENT-CONNECTED - Connection to aa completed [id=0 id_str=]"),
        Event::Connected
    );
    assert_eq!(
        parse_event("<3>CTRL-EVENT-SSID-TEMP-DISABLED id=2 ssid=\"x\" auth_failures=1 duration=10 reason=WRONG_KEY"),
        Event::TempDisabled { id: 2, reason: "WRONG_KEY".into() }
    );
    assert_eq!(parse_event("<3>CTRL-EVENT-BSS-ADDED 1 aa"), Event::Other);
}
