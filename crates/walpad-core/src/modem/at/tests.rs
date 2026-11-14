use super::worker::run_job;
use super::*;

#[test]
fn replies_are_split_from_transcripts() {
    let r = split_reply("AT+CSQ", "AT+CSQ\r\r\n+CSQ: 29, 5\r\n\r\nOK\r\n").unwrap();
    assert_eq!(r.lines, ["+CSQ: 29, 5"]);
    assert_eq!(r.fin, Final::Ok);
    assert_eq!(
        split_reply("AT+CSQ", "\r\n+CSQ: 29, 5\r\n"),
        None,
        "no final yet"
    );
    let e = split_reply("AT+CPIN?", "\r\n+CME ERROR: 10\r\n").unwrap();
    assert_eq!(e.fin, Final::Cme("10".into()));
    assert_eq!(
        split_reply("AT+CMGS", "+CMS ERROR: 500\r\n").unwrap().fin,
        Final::Cms("500".into())
    );
    assert_eq!(
        split_reply("ATD", "NO CARRIER\r\n").unwrap().fin,
        Final::NoCarrier
    );
    // URCs stay in the lines (the parsers ignore what they do not know)
    let u = split_reply("AT+CFUN?", "+EIND: 64\r\n+CFUN: 1\r\nOK\r\n").unwrap();
    assert_eq!(u.lines, ["+EIND: 64", "+CFUN: 1"]);
}

#[test]
fn a_secret_command_never_prints() {
    let c = AtCmd::secret("AT+CPIN=\"4711\"".into());
    assert_eq!(format!("{c:?}"), "AT+CPIN=***");
    assert!(!c.display().contains("4711"));
    assert_eq!(AtCmd::new("AT+CSQ").display(), "AT+CSQ");
}

#[test]
fn opening_a_missing_channel_fails_fast() {
    let r = run_job(
        Path::new("/nonexistent/radio/pttycmd1"),
        &[AtCmd::new("AT")],
        Duration::from_millis(100),
    );
    assert!(matches!(r.as_slice(), [Err(AtError::Open(_))]));
}
