use super::*;

/// A reply as `Port::command` would return it, from a recorded transcript (echo and the final
/// line are split off the same way).
fn rec(cmd: &str, transcript: &str) -> Reply {
    super::super::at::split_reply(cmd, transcript).expect("final result")
}

#[test]
fn field_splitting_keeps_quoted_commas() {
    assert_eq!(
        fields(r#"0,0,"Grameen,phone",3"#),
        ["0", "0", "Grameen,phone", "3"]
    );
    assert_eq!(fields(" 1 "), ["1"]);
}

#[test]
fn cfun_and_cpin() {
    assert_eq!(
        cfun(&rec("AT+CFUN?", "\r\n+CFUN: 0\r\n\r\nOK\r\n")),
        Some(false)
    );
    assert_eq!(
        cfun(&rec("AT+CFUN?", "AT+CFUN?\r\r\n+CFUN: 1\r\n\r\nOK\r\n")),
        Some(true)
    );
    assert_eq!(cfun(&rec("AT+CFUN?", "+CFUN: 4\r\nOK\r\n")), Some(false));
    assert_eq!(cfun(&rec("AT+CFUN?", "ERROR\r\n")), None);
    let p = |t: &str| cpin(&rec("AT+CPIN?", t));
    assert_eq!(p("\r\n+CPIN: READY\r\n\r\nOK\r\n"), SimState::Ready);
    assert_eq!(p("+CPIN: SIM PIN\r\nOK\r\n"), SimState::PinRequired);
    assert_eq!(p("+CPIN: SIM PUK\r\nOK\r\n"), SimState::PukRequired);
    assert_eq!(p("+CPIN: SIM busy\r\nOK\r\n"), SimState::Busy);
    assert_eq!(p("\r\n+CME ERROR: 10\r\n"), SimState::Absent);
    assert_eq!(p("+CME ERROR: SIM not inserted\r\n"), SimState::Absent);
    assert_eq!(p("+CME ERROR: 14\r\n"), SimState::Busy);
    assert_eq!(p("ERROR\r\n"), SimState::Unknown);
    // an unknown URC before the answer is ignored
    assert_eq!(
        p("\r\n+EIND: 64\r\n\r\n+CPIN: READY\r\n\r\nOK\r\n"),
        SimState::Ready
    );
}

#[test]
fn signal_quality() {
    assert_eq!(csq(&rec("AT+CSQ", "+CSQ: 29, 5\r\nOK\r\n")), Some((4, -55)));
    assert_eq!(csq(&rec("AT+CSQ", "+CSQ: 9,99\r\nOK\r\n")), Some((3, -95)));
    assert_eq!(csq(&rec("AT+CSQ", "+CSQ: 6,99\r\nOK\r\n")), Some((2, -101)));
    assert_eq!(csq(&rec("AT+CSQ", "+CSQ: 3,99\r\nOK\r\n")), Some((1, -107)));
    assert_eq!(csq(&rec("AT+CSQ", "+CSQ: 0,99\r\nOK\r\n")), Some((0, -113)));
    assert_eq!(csq(&rec("AT+CSQ", "+CSQ: 99,99\r\nOK\r\n")), None);
    assert_eq!(csq(&rec("AT+CSQ", "+CME ERROR: 100\r\n")), None);
    assert_eq!(csq(&rec("AT+CSQ", "+CSQ: x\r\nOK\r\n")), None);
}

#[test]
fn registration() {
    let r = |p: &str, t: &str| reg(&rec("AT+CREG?", t), p);
    assert_eq!(r("+CREG:", "+CREG: 0,1\r\nOK\r\n"), Some(1));
    assert_eq!(
        r("+CEREG:", "+CEREG: 2,5,\"1A2B\",\"01C2D3E\",7\r\nOK\r\n"),
        Some(5)
    );
    // URC (one field) and the query reply: the reply wins
    assert_eq!(r("+CREG:", "+CREG: 2\r\n+CREG: 0,1\r\nOK\r\n"), Some(1));
    assert_eq!(r("+CGREG:", "+CREG: 0,1\r\nOK\r\n"), None);
    assert_eq!(r("+CREG:", "ERROR\r\n"), None);
    assert_eq!(combine_reg(&[Some(0), Some(1), Some(2)]), MobileReg::Home);
    assert_eq!(combine_reg(&[Some(5), None, Some(2)]), MobileReg::Roaming);
    assert_eq!(combine_reg(&[Some(2), Some(0), None]), MobileReg::Searching);
    assert_eq!(combine_reg(&[Some(3), Some(0), None]), MobileReg::Denied);
    assert_eq!(combine_reg(&[Some(0), Some(4)]), MobileReg::NotRegistered);
    assert_eq!(combine_reg(&[None, None]), MobileReg::Unknown);
}

#[test]
fn operator_and_technology() {
    let c = |t: &str| cops(&rec("AT+COPS?", t));
    assert_eq!(
        c("+COPS: 0,0,\"Grameenphone\",3\r\nOK\r\n"),
        (Some("Grameenphone".into()), Some("2G"))
    );
    assert_eq!(
        c("+COPS: 0,0,\"Robi\",7\r\nOK\r\n"),
        (Some("Robi".into()), Some("4G"))
    );
    assert_eq!(
        c("+COPS: 0,0,\"banglalink\",2\r\nOK\r\n"),
        (Some("banglalink".into()), Some("3G"))
    );
    assert_eq!(c("+COPS: 0\r\nOK\r\n"), (None, None));
    // numeric format is not a name
    assert_eq!(c("+COPS: 0,2,\"47001\",7\r\nOK\r\n"), (None, Some("4G")));
    assert_eq!(c("+CME ERROR: 30\r\n"), (None, None));
}

#[test]
fn identity() {
    let a = rec(
        "ATI",
        "ATI\r\r\nManufacturer: MediaTek Inc.\r\nModel: MTK_MODEM\r\nRevision: MOLY.LR12A.R3.MP.V98.P10\r\nIMEI: 351234567890123\r\n\r\nOK\r\n",
    );
    assert_eq!(ati(&a).as_deref(), Some("MOLY.LR12A.R3.MP.V98.P10"));
    assert_eq!(ati(&rec("ATI", "MTK2\r\nOK\r\n")).as_deref(), Some("MTK2"));
    assert_eq!(
        imei(&rec("AT+CGSN", "\r\n351234567890123\r\n\r\nOK\r\n")).as_deref(),
        Some("351234567890123")
    );
    assert_eq!(
        imei(&rec("AT+CGSN", "+CGSN: \"351234567890123\"\r\nOK\r\n")).as_deref(),
        Some("351234567890123")
    );
    assert_eq!(imei(&rec("AT+CGSN", "ERROR\r\n")), None);
    assert_eq!(
        iccid(&rec("AT+CCID", "+CCID: \"8988012345678901234F\"\r\nOK\r\n")).as_deref(),
        Some("8988012345678901234F")
    );
    assert_eq!(mask("351234567890123"), "***0123");
    assert_eq!(mask("12"), "***12");
}
