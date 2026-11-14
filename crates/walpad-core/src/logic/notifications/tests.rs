use super::*;

fn cats(fired: &[NewNotif]) -> Vec<Cat> {
    fired.iter().map(|n| n.category).collect()
}

#[test]
fn no_change_fires_nothing() {
    let s = Snapshot::default();
    assert!(diff(&s, &s).is_empty());
}

#[test]
fn charger_plug_and_unplug() {
    let off = Snapshot::default();
    let on = Snapshot {
        charging: true,
        ..Snapshot::default()
    };
    let fired = diff(&off, &on);
    assert_eq!(cats(&fired), vec![Cat::Charging]);
    assert_eq!(fired[0].title, "Charger connected");
    let fired = diff(&on, &off);
    assert_eq!(fired[0].title, "Charger disconnected");
    // charging -> charging: nothing
    assert!(diff(&on, &on).is_empty());
}

#[test]
fn usb_device_attach_and_detach() {
    let empty = Snapshot::default();
    let plugged = Snapshot {
        usb_devices: vec![("1-1".into(), "Keyboard".into())],
        ..Snapshot::default()
    };
    let fired = diff(&empty, &plugged);
    assert_eq!(cats(&fired), vec![Cat::UsbDevice]);
    assert!(fired[0].body.contains("Keyboard") && fired[0].body.contains("1-1"));
    assert_eq!(fired[0].title, "USB device connected");
    let fired = diff(&plugged, &empty);
    assert_eq!(fired[0].title, "USB device disconnected");
    // same device, nothing changes
    assert!(diff(&plugged, &plugged).is_empty());
    // one more device plugged alongside an already-connected one: only the new one fires
    let two = Snapshot {
        usb_devices: vec![
            ("1-1".into(), "Keyboard".into()),
            ("1-2".into(), "Mouse".into()),
        ],
        ..Snapshot::default()
    };
    let fired = diff(&plugged, &two);
    assert_eq!(fired.len(), 1);
    assert!(fired[0].body.contains("Mouse"));
}

#[test]
fn sd_card_and_usb_disk_mount_unmount() {
    let empty = Snapshot::default();
    let sd = Snapshot {
        removable_mounts: vec!["/mnt/sd".into()],
        ..Snapshot::default()
    };
    let fired = diff(&empty, &sd);
    assert_eq!(cats(&fired), vec![Cat::Storage]);
    assert_eq!(fired[0].title, "SD card connected");
    let fired = diff(&sd, &empty);
    assert_eq!(fired[0].title, "SD card disconnected");
    let usb_disk = Snapshot {
        removable_mounts: vec!["/mnt/usb/sda1".into()],
        ..Snapshot::default()
    };
    let fired = diff(&empty, &usb_disk);
    assert_eq!(fired[0].title, "USB disk connected");
}

#[test]
fn bluetooth_connect_and_disconnect() {
    let empty = Snapshot::default();
    let conn = Snapshot {
        bt_connected: vec![("AA:BB:CC:DD:EE:FF".into(), "Headphones".into())],
        ..Snapshot::default()
    };
    let fired = diff(&empty, &conn);
    assert_eq!(cats(&fired), vec![Cat::Bluetooth]);
    assert_eq!(fired[0].title, "Bluetooth device connected");
    assert_eq!(fired[0].body, "Headphones");
    let fired = diff(&conn, &empty);
    assert_eq!(fired[0].title, "Bluetooth device disconnected");
}

#[test]
fn mobile_data_up_and_down() {
    let off = Snapshot::default();
    let on = Snapshot {
        mobile_data_on: true,
        ..Snapshot::default()
    };
    assert_eq!(diff(&off, &on)[0].title, "Mobile data connected");
    assert_eq!(diff(&on, &off)[0].title, "Mobile data disconnected");
}

#[test]
fn hotspot_client_count_up_and_down() {
    let none = Snapshot::default();
    let one = Snapshot {
        hotspot_clients: 1,
        ..Snapshot::default()
    };
    assert_eq!(diff(&none, &one)[0].title, "Hotspot client connected");
    assert_eq!(diff(&one, &none)[0].title, "Hotspot client disconnected");
    // 1 -> 2: still a connect (more clients), not a flood of per-client events
    let two = Snapshot {
        hotspot_clients: 2,
        ..Snapshot::default()
    };
    assert_eq!(diff(&one, &two).len(), 1);
}

#[test]
fn gps_fix_acquired_and_lost() {
    let searching = Snapshot::default();
    let fix = Snapshot {
        gps_fix: true,
        ..Snapshot::default()
    };
    assert_eq!(diff(&searching, &fix)[0].title, "GPS fix acquired");
    assert_eq!(diff(&fix, &searching)[0].title, "GPS fix lost");
}

#[test]
fn a_sequence_of_snapshots_fires_only_on_transitions() {
    // given a sequence of status snapshots, which notifications fire (owner's test recipe)
    let seq = [
        Snapshot::default(),
        Snapshot {
            charging: true,
            ..Snapshot::default()
        },
        Snapshot {
            charging: true,
            usb_devices: vec![("1-1".into(), "Drive".into())],
            ..Snapshot::default()
        },
        Snapshot {
            charging: true,
            usb_devices: vec![("1-1".into(), "Drive".into())],
            removable_mounts: vec!["/mnt/usb/sda1".into()],
            ..Snapshot::default()
        },
        Snapshot {
            charging: false,
            usb_devices: vec![("1-1".into(), "Drive".into())],
            removable_mounts: vec!["/mnt/usb/sda1".into()],
            ..Snapshot::default()
        },
    ];
    let mut all_titles = Vec::new();
    for w in seq.windows(2) {
        all_titles.extend(diff(&w[0], &w[1]).into_iter().map(|n| n.title));
    }
    assert_eq!(
        all_titles,
        vec![
            "Charger connected",
            "USB device connected",
            "USB disk connected",
            "Charger disconnected",
        ]
    );
}
