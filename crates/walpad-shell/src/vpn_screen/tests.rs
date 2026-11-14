use super::*;
use walpad_proto::{VpnKind, VpnStatus};

fn st(f: impl FnOnce(&mut VpnStatus)) -> Option<Status> {
    let mut s = crate::app::tests::status(120, &[]);
    f(&mut s.vpn);
    Some(s)
}
fn home(v: &mut VpnStatus) {
    v.profile = Some("home".into());
    v.kind = Some(VpnKind::Wireguard);
    v.profiles = vec![
        walpad_proto::VpnProfile {
            name: "home".into(),
            kind: VpnKind::Wireguard,
        },
        walpad_proto::VpnProfile {
            name: "work".into(),
            kind: VpnKind::Openvpn,
        },
    ];
}

#[test]
fn lines_for_every_state() {
    assert_eq!(card_lines(&None).0, "Unknown");
    let none = st(|_| {});
    assert_eq!(card_lines(&none).1, "Import a profile to start");
    assert!(!switchable(&none));
    let off = st(home);
    assert_eq!(card_lines(&off).0, "home · WireGuard");
    assert!(switchable(&off) && editable(&off) && !is_on(&off));
    let on = st(|v| {
        home(v);
        v.state = VpnState::On;
        v.handshake_age_s = Some(12);
        v.rx_bytes = Some(2048);
        v.tx_bytes = Some(10);
    });
    assert!(is_on(&on) && !editable(&on));
    assert_eq!(
        card_lines(&on).3.as_deref(),
        Some("Handshake 12 s ago · down 2.0 KB · up 10 B")
    );
    let failed = st(|v| {
        home(v);
        v.state = VpnState::Failed;
        v.reason = Some("no handshake".into());
    });
    assert_eq!(card_lines(&failed).1, "Failed: no handshake");
    assert!(editable(&failed));
    assert_eq!(state_chip(&failed).0, "Failed");
    let mid = st(|v| {
        home(v);
        v.state = VpnState::Stopping;
    });
    assert!(!switchable(&mid));
    assert_eq!(dns_value(&off), "Automatic");
    assert_eq!(proxy_value(&off), "None");
    let set = st(|v| {
        v.dns = vec!["1.1.1.1".into(), "9.9.9.9".into()];
        v.proxy = Some("http://10.0.0.2:3128".into());
    });
    assert_eq!(dns_value(&set), "1.1.1.1, 9.9.9.9");
    assert_eq!(proxy_value(&set), "http://10.0.0.2:3128");
    assert_eq!(bytes(1_500_000), "1.4 MB");
}

#[test]
fn layout_rows_are_touchable_and_do_not_overlap() {
    for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
        for (s, n) in [
            (st(home), 0),
            (st(home), 3),
            (
                st(|v| {
                    home(v);
                    v.state = VpnState::On;
                    v.revert_in_s = Some(40);
                }),
                9,
            ),
        ] {
            let l = layout(&f, &s, n);
            let mut rs = vec![
                l.title,
                l.chip,
                l.card,
                l.conn_label,
                l.switch_row,
                l.kill_row,
                l.profile_row,
                l.note,
                l.import_label,
                l.net_label,
                l.dns_row,
                l.proxy_row,
            ];
            rs.extend(l.keep);
            rs.extend(l.import_note);
            rs.extend(l.import_rows.iter().copied());
            for r in [
                l.switch_row,
                l.kill_row,
                l.profile_row,
                l.dns_row,
                l.proxy_row,
            ] {
                assert!(r.h >= TOUCH_MIN);
            }
            if let Some(b) = l.keep_btn {
                assert!(b.h >= TOUCH_MIN);
            }
            for (i, a) in rs.iter().enumerate() {
                for b in &rs[i + 1..] {
                    let o =
                        a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom();
                    assert!(!o, "{a:?} overlaps {b:?}");
                }
            }
            assert!(l.import_rows.len() <= MAX_FILES);
            assert!(l.total > l.proxy_row.bottom());
        }
    }
    assert!(layout(&Frame::new(800, 1280), &st(home), 0).keep.is_none());
}
