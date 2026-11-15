//! USB bus facts (owner request 2026-11-12, change `device-notifications`): which non-hub devices
//! are attached to the OTG port (host mode) and which removable disks `walpad-storage` has
//! mounted. Everything is a sysfs/procfs read through `Root`; a missing file or directory means
//! "none", never guessed. No existing daemon code tracked either of these before this change (see
//! `memory/hardware.md`: "USB OTG host ... not working/not attempted"), so this is new, minimal
//! detection, not a port of anything already verified on the device.
use crate::{sysfs::read_trim, Root};

/// One attached, non-hub USB device under `/sys/bus/usb/devices` (root/external hubs and
/// interface entries like `1-1:1.0` are filtered out by `attached_devices`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UsbDevice {
    /// The sysfs id, e.g. `1-1` or `1-1.2` (stable for as long as the device stays plugged in).
    pub id: String,
    pub vendor_id: Option<String>,
    pub product_id: Option<String>,
    /// `product` (falling back to `manufacturer`) sysfs attribute; never used as a path or shell
    /// argument.
    pub name: Option<String>,
}

/// bDeviceClass of a USB hub (root hubs' own `usbN` entries are class 9 too, and are filtered out
/// separately by their `usb` name prefix).
const HUB_CLASS: &str = "09";

/// Attached, non-hub USB devices, sorted by id for a stable diff. Interface entries (containing
/// `:`) and hub entries (root `usbN` or external, bDeviceClass 09) are skipped: only devices a
/// person plugged in remain.
pub fn attached_devices(root: &Root) -> Vec<UsbDevice> {
    let dir = root.p("/sys/bus/usb/devices");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<UsbDevice> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            if name.contains(':') || name.starts_with("usb") {
                return None;
            }
            let base = format!("/sys/bus/usb/devices/{name}");
            let attr = |f: &str| read_trim(root, &format!("{base}/{f}"));
            if attr("bDeviceClass").as_deref() == Some(HUB_CLASS) {
                return None;
            }
            Some(UsbDevice {
                vendor_id: attr("idVendor"),
                product_id: attr("idProduct"),
                name: attr("product").or_else(|| attr("manufacturer")),
                id: name,
            })
        })
        .collect();
    out.sort();
    out
}

/// Mountpoints `walpad-storage` keeps for the SD card (`/mnt/sd`) and USB disks (`/mnt/usb/*`),
/// read from `/proc/mounts`. Sorted and de-duplicated.
pub fn removable_mounts(root: &Root) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(root.p("/proc/mounts")) else {
        return Vec::new();
    };
    let mut out: Vec<String> = text
        .lines()
        .filter_map(|l| l.split_whitespace().nth(1))
        .filter(|mp| *mp == "/mnt/sd" || mp.starts_with("/mnt/usb/"))
        .map(str::to_string)
        .collect();
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests;
