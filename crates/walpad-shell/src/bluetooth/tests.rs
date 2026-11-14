use super::*;
use walpad_proto::{BluetoothController, BluetoothDevice, BluetoothStatus, Status};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::TOUCH_MIN;
use walpad_ui::Icon;

fn dev(a: &str, name: &str, paired: bool, connected: bool) -> BluetoothDevice {
    BluetoothDevice {
        address: a.into(),
        name: name.into(),
        paired,
        connected,
        ..Default::default()
    }
}

#[test]
fn lists_are_split_and_stably_ordered() {
    let st = crate::app::tests::status(1, &[]);
    let mut st = st;
    st.bluetooth = BluetoothStatus {
        controller: BluetoothController::Present,
        powered: true,
        devices: vec![
            dev("00:00:00:00:00:03", "zeta", true, false),
            dev("00:00:00:00:00:02", "Alpha", true, true),
            dev("00:00:00:00:00:05", "phone", false, false),
            dev("00:00:00:00:00:04", "Phone", false, false),
        ],
        ..Default::default()
    };
    let st = Some(st);
    let mine: Vec<_> = my_devices(&st).iter().map(|d| d.name.as_str()).collect();
    assert_eq!(mine, ["Alpha", "zeta"]);
    let av: Vec<_> = available_devices(&st)
        .iter()
        .map(|d| d.address.as_str())
        .collect();
    assert_eq!(av, ["00:00:00:00:00:04", "00:00:00:00:00:05"]);
    assert!(powered(&st) && switch_on(&st) && switchable(&st));
    assert_eq!(qs_line(&st).0, "Connected: Alpha");
}

#[test]
fn controller_states_map_to_lines() {
    let mut s = crate::app::tests::status(1, &[]);
    let line = |s: &Status, f: &dyn Fn(&mut BluetoothStatus)| {
        let mut s = s.clone();
        f(&mut s.bluetooth);
        let o = Some(s);
        (qs_line(&o).0, chip(&o).0, switch_on(&o), switchable(&o))
    };
    assert_eq!(
        line(&s, &|_| {}),
        ("Not started".into(), "Stopped", false, true)
    );
    assert_eq!(
        line(&s, &|b| b.enabled = false),
        ("Off".into(), "Off", false, true)
    );
    assert_eq!(
        line(&s, &|b| b.controller = BluetoothController::Starting),
        ("Starting...".into(), "Starting", true, false)
    );
    assert_eq!(
        line(&s, &|b| b.reason =
            Some("Bluetooth is not installed on this system".into())),
        ("Not available".into(), "None", false, false)
    );
    assert_eq!(
        line(&s, &|b| b.reason =
            Some("bridge did not create an HCI device".into())),
        ("Failed to start".into(), "Failed", false, true)
    );
    assert_eq!(
        line(&s, &|b| {
            b.controller = BluetoothController::Present;
            b.powered = true;
            b.discovering = true;
        }),
        ("Scanning...".into(), "On", true, true)
    );
    assert_eq!(
        line(&s, &|b| b.controller = BluetoothController::Present),
        ("Off".into(), "Off", false, true)
    );
    s.bluetooth.controller = BluetoothController::Present;
    assert!(!powered(&Some(s)));
}

#[test]
fn touch_targets_fit_in_both_orientations() {
    let mut s = crate::app::tests::status(1, &[]);
    s.bluetooth = BluetoothStatus {
        controller: BluetoothController::Present,
        powered: true,
        devices: vec![
            dev("00:00:00:00:00:02", "Alpha", true, true),
            dev("00:00:00:00:00:04", "Phone", false, false),
        ],
        ..Default::default()
    };
    let st = Some(s);
    for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
        let l = layout(&f, &st);
        let c = f.padded();
        assert!(l.scan_btn.h >= TOUCH_MIN && l.scan_btn.w >= TOUCH_MIN);
        for r in l.mine_rows.iter().chain(&l.avail_rows) {
            assert!(r.row.h >= TOUCH_MIN && r.row.x >= c.x && r.row.right() <= c.right());
            if let Some(fg) = r.forget {
                assert!(fg.w >= 80 && fg.h >= TOUCH_MIN && fg.right() <= r.row.right());
            }
        }
        assert!(l.avail_card.y > l.mine_card.bottom());
        assert!(l.total > l.avail_card.bottom());
    }
    // off: one state card, no lists
    let off = Some(crate::app::tests::status(1, &[]));
    let l = layout(&Frame::new(800, 1280), &off);
    assert!(l.state_card.is_some() && l.mine_rows.is_empty());
}

#[test]
fn rssi_bars_and_icons() {
    assert_eq!(bars_from_rssi(Some(-50)), 4);
    assert_eq!(bars_from_rssi(Some(-90)), 1);
    assert_eq!(bars_from_rssi(None), 0);
    let mut d = dev("00:00:00:00:00:02", "k", true, false);
    d.icon = Some("input-keyboard".into());
    assert_eq!(device_icon(&d), Icon::Keyboard);
    d.icon = Some("audio-headset".into());
    assert_eq!(device_icon(&d), Icon::Bluetooth);
}
