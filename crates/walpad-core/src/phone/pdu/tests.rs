use super::deliver::scts_ms;
use super::*;

#[test]
fn septets_pack_like_the_spec_example() {
    // 3GPP TS 23.038 example: "hellohello" -> E8329BFD4697D9EC37
    let s = gsm7_encode("hellohello").unwrap();
    assert_eq!(hex_encode(&pack7(&s, 0)), "E8329BFD4697D9EC37");
    assert_eq!(gsm7_decode(&unpack7(&pack7(&s, 0), 10, 0)), "hellohello");
}

#[test]
fn gsm7_alphabet_and_extension() {
    assert_eq!(gsm7_encode("A@£$").unwrap(), [0x41, 0x00, 0x01, 0x02]);
    // euro and brackets need an escape: two septets each
    assert_eq!(gsm7_encode("€").unwrap(), [0x1B, 0x65]);
    assert_eq!(
        gsm7_decode(&gsm7_encode("a[b]{c}~\\|^€").unwrap()),
        "a[b]{c}~\\|^€"
    );
    // Bangla and emoji are not GSM
    assert!(gsm7_encode("আমি").is_none());
    assert!(gsm7_encode("hi 😀").is_none());
    assert_eq!(alphabet("hello"), Alphabet::Gsm7);
    assert_eq!(alphabet("আমি"), Alphabet::Ucs2);
}

#[test]
fn ucs2_round_trips_bangla_and_emoji() {
    let t = "আমি বাংলায় গান গাই 😀";
    assert_eq!(ucs2_decode(&ucs2_encode(t)), t);
    assert_eq!(hex_encode(&ucs2_encode("আ")), "0986");
}

#[test]
fn submit_single_gsm7_matches_a_known_pdu() {
    let p = encode_submit(None, "+447700900123", "hellohello", 0).unwrap();
    assert_eq!(p.len(), 1);
    assert_eq!(
        p[0].hex,
        "001100".to_string() + "0C91447700091032" + "0000AA0A" + "E8329BFD4697D9EC37"
    );
    // TPDU = everything after the one-octet SMSC field
    assert_eq!(p[0].tpdu_len, p[0].hex.len() / 2 - 1);
}

#[test]
fn submit_national_number_and_smsc() {
    let p = encode_submit(Some("+8801716000000"), "01711234567", "Hi", 1).unwrap();
    let h = &p[0].hex;
    assert!(h.starts_with("0891881017060000F0"), "{h}");
    assert!(h.contains("0B811017214365F7"), "{h}");
    assert_eq!(p[0].tpdu_len, h.len() / 2 - 9);
    // a broken SMSC falls back to the default
    let q = encode_submit(Some("12ab"), "01711234567", "Hi", 1).unwrap();
    assert!(q[0].hex.starts_with("0011"));
}

#[test]
fn submit_ucs2_for_bangla() {
    let p = encode_submit(None, "01711234567", "আমি", 0).unwrap();
    assert_eq!(p.len(), 1);
    let h = &p[0].hex;
    assert!(h.contains("0008AA06098609AE09BF"), "{h}");
    assert_eq!(deliver_from_submit(h).text, "আমি");
}

/// Turn a SUBMIT PDU into the matching DELIVER (same address and data) to test both ways.
fn deliver_from_submit(h: &str) -> Deliver {
    let b = hex_decode(h).unwrap();
    let mut i = 1 + b[0] as usize;
    let first = b[i];
    i += 2; // first octet, message reference
    let da_len = b[i] as usize;
    let oa_end = i + 2 + da_len.div_ceil(2);
    let oa = &b[i..oa_end];
    let (pid, dcs) = (b[oa_end], b[oa_end + 1]);
    let ud = &b[oa_end + 3..]; // skip the validity octet
    let mut d = vec![0x00, first & 0x40];
    d.extend(oa);
    d.extend([pid, dcs]);
    d.extend([0x11, 0x21, 0x31, 0x21, 0x43, 0x65, 0x23]);
    d.extend(ud);
    decode_deliver(&hex_encode(&d)).unwrap()
}

#[test]
fn long_messages_are_split_with_concat_headers() {
    // 160 septets fit in one SMS, 161 need two parts of up to 153
    let t160 = "a".repeat(160);
    assert_eq!(
        encode_submit(None, "01711234567", &t160, 7).unwrap().len(),
        1
    );
    let t161 = "a".repeat(161);
    let p = encode_submit(None, "01711234567", &t161, 7).unwrap();
    assert_eq!(p.len(), 2);
    // UDH 050003 07 02 01 / 02 right after UDL
    assert!(p[0].hex.contains("050003070201"), "{}", p[0].hex);
    assert!(p[1].hex.contains("050003070202"), "{}", p[1].hex);
    // UCS2: 70 units single, 71 need two parts of up to 67
    let b70 = "আ".repeat(70);
    assert_eq!(
        encode_submit(None, "01711234567", &b70, 1).unwrap().len(),
        1
    );
    let b71 = "আ".repeat(71);
    assert_eq!(
        encode_submit(None, "01711234567", &b71, 1).unwrap().len(),
        2
    );
    // the cap: 6 parts
    let ok = "a".repeat(PART_GSM7 * MAX_PARTS);
    assert_eq!(
        encode_submit(None, "01711234567", &ok, 1).unwrap().len(),
        MAX_PARTS
    );
    let too = "a".repeat(PART_GSM7 * MAX_PARTS + 1);
    assert_eq!(
        encode_submit(None, "01711234567", &too, 1),
        Err(EncodeError::TooLong)
    );
    assert_eq!(
        encode_submit(None, "01711234567", "", 1),
        Err(EncodeError::TooLong)
    );
    assert_eq!(
        encode_submit(None, "01x", "a", 1),
        Err(EncodeError::BadNumber)
    );
}

#[test]
fn an_escape_pair_is_never_cut_between_parts() {
    // 152 'a' then a euro (2 septets) would straddle the 153 boundary
    let t = format!("{}€{}", "a".repeat(152), "b".repeat(20));
    let p = encode_submit(None, "01711234567", &t, 3).unwrap();
    assert_eq!(p.len(), 2);
    let d1 = part_text(&p[0]);
    let d2 = part_text(&p[1]);
    assert_eq!(format!("{d1}{d2}"), t);
    assert_eq!(d1.chars().count(), 152);
}

fn part_text(p: &Part) -> String {
    deliver_from_submit(&p.hex).text
}

#[test]
fn concatenated_gsm7_decodes_back_in_order() {
    let t: String = (0..400)
        .map(|i| char::from(b'a' + (i % 26) as u8))
        .collect();
    let parts = encode_submit(None, "+8801711234567", &t, 9).unwrap();
    assert_eq!(parts.len(), 3);
    let mut got = String::new();
    for (i, p) in parts.iter().enumerate() {
        let d = deliver_from_submit(&p.hex);
        assert_eq!(d.concat, Some((9, 3, (i + 1) as u8)));
        assert_eq!(d.sender, "+8801711234567");
        got.push_str(&d.text);
    }
    assert_eq!(got, t);
}

#[test]
fn concatenated_ucs2_decodes_back() {
    let t = "আমার সোনার বাংলা ".repeat(8);
    let parts = encode_submit(None, "01711234567", &t, 2).unwrap();
    assert!(parts.len() >= 2);
    let got: String = parts
        .iter()
        .map(|p| deliver_from_submit(&p.hex).text)
        .collect();
    assert_eq!(got, t);
}

#[test]
fn deliver_from_the_wild() {
    // a widely quoted example: sender +31641600986, "How are you?"
    let d = decode_deliver(
        "07911326040000F0040B911346610089F60040208062917314080CC8F71D14969741F977FD07",
    )
    .unwrap();
    assert_eq!(d.sender, "+31641600986");
    assert_eq!(d.text, "How are you?");
    assert!(d.ts_ms.is_some());
    assert_eq!(d.concat, None);
}

#[test]
fn alphanumeric_sender_and_status_reports() {
    let sender = hex_encode(&pack7(&gsm7_encode("Dhaka").unwrap(), 0));
    assert_eq!(sender.len(), 10);
    let ud = hex_encode(&pack7(&gsm7_encode("ok").unwrap(), 0));
    let pdu = format!("00040AD0{sender}0000112131415263 0002{ud}").replace(' ', "");
    let d = decode_deliver(&pdu).unwrap();
    assert_eq!(d.sender, "Dhaka");
    assert_eq!(d.text, "ok");
    // SMS-STATUS-REPORT (MTI 2) and garbage are not messages
    assert_eq!(decode_deliver("0006010C910000"), None);
    assert_eq!(decode_deliver("zz"), None);
    assert_eq!(decode_deliver(""), None);
    assert_eq!(decode_deliver("0004"), None);
}

#[test]
fn timestamps_use_the_zone() {
    // 2020-05-17 10:30:45 at +6:00 (24 quarters = 0x42 -> swapped nibbles "42") = 04:30:45 UTC
    let b = [0x02, 0x50, 0x71, 0x01, 0x03, 0x54, 0x42];
    let ms = scts_ms(&b).unwrap();
    // 2020-05-17T04:30:45Z
    assert_eq!(ms / 1000, 1_589_689_845);
    assert_eq!(
        scts_ms(&[0x02, 0x50, 0x71, 0x01, 0x03, 0x54, 0x4A]).map(|m| m / 1000),
        Some(1_589_689_845 + 43_200)
    );
    assert_eq!(scts_ms(&[0xFF; 7]), None);
}

#[test]
fn hex_helpers_reject_garbage() {
    assert_eq!(hex_decode("0A1b"), Some(vec![0x0A, 0x1B]));
    assert_eq!(hex_decode("0A1"), None);
    assert_eq!(hex_decode("0G"), None);
    assert_eq!(hex_decode("é1"), None);
}
