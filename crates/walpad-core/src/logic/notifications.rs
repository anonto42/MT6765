//! Pure notification-generation logic (owner request 2026-11-12, change `device-notifications`):
//! "a notification type system that it will notify every events like charging, any otg or other
//! things was connected". Given the previous and current snapshot of the few signals the daemon
//! already tracks (battery, Bluetooth, mobile data, hotspot, GPS) plus the new USB/OTG and
//! removable-storage sysfs reads (`walpad_hw::usb`), decide which notifications fire. No I/O here:
//! everything is testable with synthetic snapshots.
use walpad_proto::NotificationCategory as Cat;

/// One signal snapshot at a point in time. `usb_devices` is `(sysfs id, display name)` of
/// attached non-hub USB/OTG devices; `bt_connected` is `(address, name)` of connected Bluetooth
/// devices. Both are compared by their first element (id / address); sort order does not matter.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub charging: bool,
    pub usb_devices: Vec<(String, String)>,
    pub removable_mounts: Vec<String>,
    pub bt_connected: Vec<(String, String)>,
    pub mobile_data_on: bool,
    pub hotspot_clients: u32,
    pub gps_fix: bool,
}

/// What a fired notification says, before it gets an id/timestamp/read flag in the store.
#[derive(Debug, Clone, PartialEq)]
pub struct NewNotif {
    pub category: Cat,
    pub title: String,
    pub body: String,
}

fn notif(category: Cat, title: &str, body: impl Into<String>) -> NewNotif {
    NewNotif {
        category,
        title: title.to_string(),
        body: body.into(),
    }
}

/// Mountpoint `walpad-storage` uses -> a human label.
fn storage_label(mountpoint: &str) -> &'static str {
    if mountpoint == "/mnt/sd" {
        "SD card"
    } else {
        "USB disk"
    }
}

/// What should fire going from `prev` to `cur`. The caller (`crate::notifications::Notifications`)
/// only calls this from the second observation onward, so a daemon restart never floods the
/// history with "just connected" for everything already plugged in.
pub fn diff(prev: &Snapshot, cur: &Snapshot) -> Vec<NewNotif> {
    let mut out = Vec::new();

    if cur.charging && !prev.charging {
        out.push(notif(Cat::Charging, "Charger connected", "Now charging."));
    } else if !cur.charging && prev.charging {
        out.push(notif(
            Cat::Charging,
            "Charger disconnected",
            "Running on battery.",
        ));
    }

    for (id, name) in &cur.usb_devices {
        if !prev.usb_devices.iter().any(|(pid, _)| pid == id) {
            out.push(notif(
                Cat::UsbDevice,
                "USB device connected",
                format!("{name} ({id})"),
            ));
        }
    }
    for (id, name) in &prev.usb_devices {
        if !cur.usb_devices.iter().any(|(cid, _)| cid == id) {
            out.push(notif(
                Cat::UsbDevice,
                "USB device disconnected",
                format!("{name} ({id})"),
            ));
        }
    }

    for mp in &cur.removable_mounts {
        if !prev.removable_mounts.contains(mp) {
            let what = storage_label(mp);
            out.push(notif(
                Cat::Storage,
                &format!("{what} connected"),
                format!("Mounted at {mp}."),
            ));
        }
    }
    for mp in &prev.removable_mounts {
        if !cur.removable_mounts.contains(mp) {
            let what = storage_label(mp);
            out.push(notif(
                Cat::Storage,
                &format!("{what} disconnected"),
                format!("Unmounted from {mp}."),
            ));
        }
    }

    for (addr, name) in &cur.bt_connected {
        if !prev.bt_connected.iter().any(|(pa, _)| pa == addr) {
            out.push(notif(Cat::Bluetooth, "Bluetooth device connected", name));
        }
    }
    for (addr, name) in &prev.bt_connected {
        if !cur.bt_connected.iter().any(|(ca, _)| ca == addr) {
            out.push(notif(Cat::Bluetooth, "Bluetooth device disconnected", name));
        }
    }

    if cur.mobile_data_on && !prev.mobile_data_on {
        out.push(notif(
            Cat::MobileData,
            "Mobile data connected",
            "The data session is up.",
        ));
    } else if !cur.mobile_data_on && prev.mobile_data_on {
        out.push(notif(
            Cat::MobileData,
            "Mobile data disconnected",
            "The data session ended.",
        ));
    }

    if cur.hotspot_clients > prev.hotspot_clients {
        out.push(notif(
            Cat::Hotspot,
            "Hotspot client connected",
            format!("{} device(s) connected now.", cur.hotspot_clients),
        ));
    } else if cur.hotspot_clients < prev.hotspot_clients {
        out.push(notif(
            Cat::Hotspot,
            "Hotspot client disconnected",
            format!("{} device(s) connected now.", cur.hotspot_clients),
        ));
    }

    if cur.gps_fix && !prev.gps_fix {
        out.push(notif(
            Cat::Gps,
            "GPS fix acquired",
            "Location is available.",
        ));
    } else if !cur.gps_fix && prev.gps_fix {
        out.push(notif(
            Cat::Gps,
            "GPS fix lost",
            "Searching for satellites again.",
        ));
    }

    out
}

#[cfg(test)]
mod tests;
