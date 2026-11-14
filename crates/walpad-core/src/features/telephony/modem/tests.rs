use super::at::{AtError, Final, Outcome, Reply};
use super::types::{field, pin_error};
use super::*;
use std::time::Instant;
use walpad_hw::Root;
use walpad_proto::{MobileReg, ModemState, Secret, SimState};

#[test]
fn conf_and_output_fields() {
    assert_eq!(parse_conf(""), None);
    assert_eq!(
        parse_conf("# x\napn=gpinternet\n").as_deref(),
        Some("gpinternet")
    );
    assert_eq!(parse_conf("apn=bad apn\n"), None);
    let out = "[modem] data on\nstate=failed\nreason=data link not supported yet\nip=10.1.2.3\n";
    assert_eq!(field(out, "state"), Some("failed"));
    assert_eq!(
        Modem::reason_of(out, Some(1)),
        "data link not supported yet"
    );
    assert_eq!(
        Modem::reason_of("[modem] after 60s: md_en=0\n", Some(0)),
        "after 60s: md_en=0"
    );
    assert_eq!(
        Modem::reason_of("", Some(3)),
        "walpad-modem failed (exit 3)"
    );
}

#[test]
fn pin_errors_are_friendly() {
    assert_eq!(pin_error("16"), "Wrong PIN");
    assert_eq!(pin_error("incorrect password"), "Wrong PIN");
    assert_eq!(pin_error("12"), "SIM locked: PUK required");
    assert_eq!(pin_error("100"), "PIN not accepted (error 100)");
}

fn reply(lines: &[&str], fin: Final) -> Outcome {
    Ok(Reply {
        lines: lines.iter().map(|s| s.to_string()).collect(),
        fin,
    })
}

fn ready_modem() -> Modem {
    let d = std::env::temp_dir().join(format!("walpad-modem-unit-{}", std::process::id()));
    let mut m = Modem::new(&Root::new(&d));
    m.st.modem = ModemState::Ready;
    m
}

#[test]
fn polls_build_from_the_known_state_and_results_fill_the_status() {
    let now = Instant::now();
    let mut m = ready_modem();
    let j = m.poll_job();
    assert_eq!(
        j.kind,
        JobKind::Poll {
            ccid: false,
            full: false
        }
    );
    assert_eq!(j.cmds.len(), 2);
    // radio off at boot, SIM ready: a quick follow-up poll, the radio switch is queued
    m.handle_poll(
        &[
            reply(&["+CFUN: 0"], Final::Ok),
            reply(&["+CPIN: READY"], Final::Ok),
        ],
        false,
        false,
        now,
    );
    assert_eq!(m.st.radio_on, Some(false));
    assert_eq!(m.st.sim, SimState::Ready);
    assert_eq!(m.st.registration, MobileReg::NotRegistered);
    assert_eq!(m.next_poll, now, "SIM changed: poll again at once");
    assert!(matches!(
        m.queue.back().map(|j| j.kind),
        Some(JobKind::Radio(true))
    ));
    m.st.radio_on = Some(true);
    let j = m.poll_job();
    assert_eq!(
        j.kind,
        JobKind::Poll {
            ccid: true,
            full: true
        }
    );
    assert_eq!(j.cmds.len(), 9);
    m.handle_poll(
        &[
            reply(&["+CFUN: 1"], Final::Ok),
            reply(&["+EIND: 64", "+CPIN: READY"], Final::Ok),
            reply(&["+CCID: \"8988012345678901234F\""], Final::Ok),
            reply(&["+CSQ: 29, 5"], Final::Ok),
            reply(&["+CREG: 0,1"], Final::Ok),
            reply(&["+CGREG: 0,1"], Final::Ok),
            reply(&["+CEREG: 0,1"], Final::Ok),
            reply(&[], Final::Ok),
            reply(&["+COPS: 0,0,\"Grameenphone\",3"], Final::Ok),
        ],
        true,
        true,
        now,
    );
    assert_eq!(m.st.signal_bars, Some(4));
    assert_eq!(m.st.signal_dbm, Some(-55));
    assert_eq!(m.st.registration, MobileReg::Home);
    assert_eq!(m.st.operator.as_deref(), Some("Grameenphone"));
    assert_eq!(m.st.tech.as_deref(), Some("2G"));
    assert!(m.iccid.is_some());
    assert_eq!(m.next_poll, now + m.t.poll_on);
    // the SIM is pulled: the network facts go, the ICCID is forgotten
    m.handle_poll(
        &[
            reply(&["+CFUN: 1"], Final::Ok),
            reply(&[], Final::Cme("10".into())),
        ],
        false,
        false,
        now,
    );
    assert_eq!(m.st.sim, SimState::Absent);
    assert_eq!(m.st.operator, None);
    assert_eq!(m.st.signal_bars, None);
    assert!(m.iccid.is_none());
    // a timeout half way keeps what is known and stays honest about the rest
    m.st.sim = SimState::Ready;
    m.handle_poll(
        &[reply(&["+CFUN: 1"], Final::Ok), Err(AtError::Timeout)],
        false,
        true,
        now,
    );
    assert_eq!(m.st.sim, SimState::Ready);
    assert_eq!(m.st.signal_bars, None);
    assert_eq!(m.st.registration, MobileReg::Unknown);
}

#[test]
fn data_needs_a_ready_modem_radio_and_sim() {
    let now = Instant::now();
    let mut m = ready_modem();
    m.st.modem = ModemState::Present;
    assert!(matches!(
        m.set_data(true, now),
        Err(MobileError::Refused(_))
    ));
    m.st.modem = ModemState::Ready;
    m.st.radio_on = Some(false);
    assert!(matches!(
        m.set_data(true, now),
        Err(MobileError::Refused(_))
    ));
    m.st.radio_on = Some(true);
    m.st.sim = SimState::PinRequired;
    assert!(matches!(
        m.set_data(true, now),
        Err(MobileError::Refused(_))
    ));
    m.st.sim = SimState::Ready;
    m.inflight = true;
    assert_eq!(m.set_data(true, now).unwrap()["data"], "connecting");
    assert_eq!(m.pending_data, Some(true));
    assert!(matches!(m.set_data(false, now), Err(MobileError::Busy(_))));
}

#[test]
fn pin_is_only_sent_when_the_sim_asks_and_never_printed() {
    let now = Instant::now();
    let mut m = ready_modem();
    let pin = Secret::new("4711");
    m.st.sim = SimState::Ready;
    assert!(matches!(m.sim_pin(&pin, now), Err(MobileError::Refused(_))));
    m.st.sim = SimState::PukRequired;
    assert!(matches!(m.sim_pin(&pin, now), Err(MobileError::Refused(_))));
    m.st.sim = SimState::PinRequired;
    m.inflight = true; // keep it queued
    assert!(m.sim_pin(&pin, now).is_ok());
    assert!(matches!(m.sim_pin(&pin, now), Err(MobileError::Busy(_))));
    let dbg = format!("{:?}", m.queue.back().map(|j| &j.cmds));
    assert!(
        dbg.contains("AT+CPIN=***") && !dbg.contains("4711"),
        "{dbg}"
    );
    assert!(matches!(
        m.sim_pin(&Secret::new("12\"4"), now),
        Err(MobileError::Invalid(_))
    ));
}
