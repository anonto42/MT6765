use super::*;
use walpad_proto::{MobileReg, MobileStatus, SimState};

fn st(f: impl FnOnce(&mut MobileStatus)) -> Option<Status> {
    let mut s = crate::app::tests::status(120, &[]);
    f(&mut s.mobile);
    Some(s)
}
fn registered_gp(m: &mut MobileStatus) {
    m.modem = ModemState::Ready;
    m.radio_on = Some(true);
    m.sim = SimState::Ready;
    m.registration = MobileReg::Home;
    m.operator = Some("Grameenphone".into());
    m.tech = Some("4G".into());
    m.signal_bars = Some(3);
    m.signal_dbm = Some(-71);
}

#[test]
fn lines_for_every_state() {
    assert_eq!(qs_line(&None).0, "Unknown");
    assert_eq!(qs_line(&st(|_| {})).0, "Not available");
    assert_eq!(
        qs_line(&st(|m| m.modem = ModemState::Booting)).0,
        "Modem starting..."
    );
    let gp = st(registered_gp);
    assert_eq!(qs_line(&gp).0, "Grameenphone · 4G");
    assert_eq!(sim_lines(&gp).0, "Grameenphone");
    assert_eq!(sim_lines(&gp).1, "4G · Registered");
    assert_eq!(pill_cell(&gp), Some((3, TEXT)));
    let pin = st(|m| {
        m.modem = ModemState::Ready;
        m.sim = SimState::PinRequired;
    });
    assert_eq!(qs_line(&pin).0, "SIM PIN required");
    assert!(pin_needed(&pin));
    assert_eq!(pill_cell(&pin), Some((0, TEXT_DISABLED)));
    let nosim = st(|m| {
        m.modem = ModemState::Ready;
        m.sim = SimState::Absent;
    });
    assert_eq!(pill_cell(&nosim), None, "no SIM: no bars in the pill");
    assert_eq!(sim_lines(&nosim).0, "No SIM card");
    let off = st(|m| {
        registered_gp(m);
        m.radio_on = Some(false);
    });
    assert_eq!(qs_line(&off).0, "Airplane mode");
    assert!(!data_switchable(&off));
    let failed = st(|m| {
        registered_gp(m);
        m.data = MobileData::Failed;
        m.data_reason = Some("data link not supported yet".into());
    });
    assert_eq!(data_line(&failed).0, "Failed: data link not supported yet");
    assert!(data_switchable(&failed) && !data_on(&failed));
    let mfail = st(|m| {
        m.modem = ModemState::Failed;
        m.reason = Some("the modem did not start".into());
    });
    assert_eq!(sim_lines(&mfail).1, "the modem did not start");
}

#[test]
fn imei_is_hidden_until_asked() {
    let info = Some(MobileInfo {
        imei: Some("351234567890123".into()),
        iccid: None,
    });
    assert_eq!(imei_value(&info, false), "Hidden");
    assert_eq!(imei_value(&info, true), "351234567890123");
    assert_eq!(imei_value(&None, true), "Loading...");
}

#[test]
fn layout_rows_are_touchable_and_do_not_overlap() {
    for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
        for s in [
            st(registered_gp),
            st(|m| {
                m.modem = ModemState::Ready;
                m.sim = SimState::PinRequired;
            }),
        ] {
            let l = layout(&f, &s);
            let mut rs = vec![
                l.title,
                l.chip,
                l.sim_card,
                l.conn_label,
                l.radio_row,
                l.data_row,
                l.apn_row,
                l.note,
                l.modem_label,
                l.firmware_row,
                l.imei_row,
            ];
            rs.extend(l.pin_btn);
            for r in [l.radio_row, l.data_row, l.apn_row, l.imei_row] {
                assert!(r.h >= TOUCH_MIN);
            }
            for (i, a) in rs.iter().enumerate() {
                for b in &rs[i + 1..] {
                    let o =
                        a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom();
                    assert!(!o, "{a:?} overlaps {b:?}");
                }
            }
            assert!(l.total > l.imei_row.bottom());
        }
    }
    assert!(layout(&Frame::new(800, 1280), &st(registered_gp))
        .pin_btn
        .is_none());
}
