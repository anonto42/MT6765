use super::*;

fn l(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn clcc_lists_calls_of_every_state() {
    let c = clcc(&l(&[
        "+CLCC: 1,1,4,0,0,\"01711234567\",129",
        "+CLCC: 2,0,0,0,0,\"+8801811000000\",145,\"\"",
        "+CLCC: 3,0,2,0,0",
        "+CLCC: 4,1,5,0,0,\"999\",129",
        "+CLCC: 5,0,3,0,0,\"112\",129",
        "+CLCC: 6,0,1,0,0,\"12\",129",
    ]));
    assert_eq!(c.len(), 6);
    assert_eq!(c[0].stat, ClccStat::Incoming);
    assert!(c[0].incoming);
    assert_eq!(c[0].number.as_deref(), Some("01711234567"));
    assert_eq!(c[1].stat, ClccStat::Active);
    assert_eq!(c[1].number.as_deref(), Some("+8801811000000"));
    assert_eq!(c[2].number, None);
    assert_eq!(c[3].stat, ClccStat::Waiting);
    assert_eq!(c[4].stat, ClccStat::Alerting);
    assert_eq!(c[5].stat, ClccStat::Held);
}

#[test]
fn clcc_ignores_data_calls_and_noise() {
    let c = clcc(&l(&[
        "+EIND: 64",
        "+CLCC: 1,0,0,1,0,\"123\",129",
        "+CLCC: x",
        "+CLCC: 2,0,9,0,0",
        "RING",
    ]));
    assert!(c.is_empty());
}

#[test]
fn urc_helpers() {
    assert!(rang(&l(&["RING"])));
    assert!(rang(&l(&["+CRING: VOICE"])));
    assert_eq!(
        clip(&l(&["RING", "+CLIP: \"01711234567\",129,\"\",,\"\",0"])).as_deref(),
        Some("01711234567")
    );
    assert_eq!(clip(&l(&["+CLIP: \"\",128"])), None);
    assert!(new_sms_urc(&l(&["+CMTI: \"SM\",3"])));
    assert!(!new_sms_urc(&l(&["+CSQ: 1,2"])));
}

#[test]
fn ecpi_disconnect_cause() {
    let busy = l(&["+ECPI: 1,6,1,0,0,0,\"\",0,\"\",17"]);
    assert_eq!(ecpi_cause(&busy), Some(17));
    assert_eq!(cause_reason(17), "busy");
    assert_eq!(cause_reason(19), "no_answer");
    assert_eq!(cause_reason(21), "rejected");
    assert_eq!(cause_reason(16), "remote");
    // other ECPI messages (alerting 2, connected 3) carry no cause
    assert_eq!(ecpi_cause(&l(&["+ECPI: 1,2,1,0,0,0,\"\",0"])), None);
}

#[test]
fn cmgl_pairs_headers_with_pdus() {
    let m = cmgl(&l(&[
        "+CMGL: 1,0,,23",
        "0791448720003023240DD0E474D81C0EBB010000111011311640000A E8329BFD4697D9EC37"
            .replace(' ', "")
            .as_str(),
        "+CMGL: 2,1,,10",
        "00AABB",
        "+CMGL: 3,3,,5",
    ]));
    assert_eq!(m.len(), 2);
    assert_eq!((m[0].index, m[0].stat), (1, 0));
    assert_eq!((m[1].index, m[1].stat), (2, 1));
    assert_eq!(m[1].pdu, "00AABB");
}

#[test]
fn csca_needs_valid_digits() {
    assert_eq!(
        csca(&l(&["+CSCA: \"+8801716000000\",145"])).as_deref(),
        Some("+8801716000000")
    );
    assert_eq!(csca(&l(&["+CSCA: \"\",128"])), None);
    assert_eq!(csca(&l(&["+CSCA: \"12;rm\",128"])), None);
    assert_eq!(csca(&l(&[])), None);
}

#[test]
fn cusd_parses_ascii_and_ucs2() {
    let a = cusd(&l(&["+CUSD: 0,\"Balance: 45.50 Tk\",15"])).unwrap();
    assert_eq!(
        (a.m, a.text.as_deref(), a.dcs),
        (0, Some("Balance: 45.50 Tk"), 15)
    );
    let u = cusd(&l(&["+CUSD: 1,\"098609A809C1\",72"])).unwrap();
    assert_eq!((u.m, u.text.as_deref(), u.dcs), (1, Some("আনু"), 72));
    let m = cusd(&l(&["+CUSD: 2, \"MSISDN:", "8801305408813\", 15"])).unwrap();
    assert_eq!(
        (m.m, m.text.as_deref(), m.dcs),
        (2, Some("MSISDN:\n8801305408813"), 15)
    );
}

#[test]
fn cpbr_parses_range_and_contacts() {
    assert_eq!(cpbr_range(&l(&["+CPBR: (1-250),40,14"])), Some((1, 250)));
    assert_eq!(cpbr_range(&l(&["+CPBR: (1-999),40,14"])), Some((1, 500)));
    let list = cpbr(&l(&[
        "+CPBR: 1,\"+8801711234567\",145,\"Rahim\"",
        "+CPBR: 2,\"01811000000\",129,\"\"",
        "+CPBR: 3,\"01911000000\",129,\"0041006D0069\"",
    ]));
    assert_eq!(list.len(), 3);
    assert_eq!(
        (list[0].name.as_str(), list[0].number.as_str()),
        ("Rahim", "+8801711234567")
    );
    assert_eq!(
        (list[1].name.as_str(), list[1].number.as_str()),
        ("01811000000", "01811000000")
    );
    assert_eq!(
        (list[2].name.as_str(), list[2].number.as_str()),
        ("Ami", "01911000000")
    );
}
