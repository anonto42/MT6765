use super::*;
use crate::testutil::{put, tmp};

#[test]
fn attached_devices_skips_hubs_and_interfaces() {
    let r = Root::new(tmp());
    // root hub: skipped (name prefix "usb")
    put(&r, "/sys/bus/usb/devices/usb1/bDeviceClass", "09\n");
    // external hub: skipped (bDeviceClass 09)
    put(&r, "/sys/bus/usb/devices/1-1/bDeviceClass", "09\n");
    // an interface of a real device: skipped (name has ':')
    put(&r, "/sys/bus/usb/devices/1-2:1.0/bInterfaceClass", "08\n");
    // a real device: a USB flash drive
    put(&r, "/sys/bus/usb/devices/1-2/bDeviceClass", "00\n");
    put(&r, "/sys/bus/usb/devices/1-2/idVendor", "0781\n");
    put(&r, "/sys/bus/usb/devices/1-2/idProduct", "5567\n");
    put(&r, "/sys/bus/usb/devices/1-2/product", "Cruzer Blade\n");
    let devs = attached_devices(&r);
    assert_eq!(
        devs,
        vec![UsbDevice {
            id: "1-2".into(),
            vendor_id: Some("0781".into()),
            product_id: Some("5567".into()),
            name: Some("Cruzer Blade".into()),
        }]
    );
    assert!(attached_devices(&Root::new(tmp())).is_empty());
}

#[test]
fn attached_device_falls_back_to_manufacturer() {
    let r = Root::new(tmp());
    put(&r, "/sys/bus/usb/devices/1-1/bDeviceClass", "00\n");
    put(&r, "/sys/bus/usb/devices/1-1/manufacturer", "Logitech\n");
    assert_eq!(attached_devices(&r)[0].name.as_deref(), Some("Logitech"));
}

#[test]
fn removable_mounts_filters_and_sorts() {
    let r = Root::new(tmp());
    put(
        &r,
        "/proc/mounts",
        "rootfs / ext4 rw 0 0\n/dev/mmcblk1p1 /mnt/sd vfat rw 0 0\n/dev/sda1 /mnt/usb/sda1 exfat rw 0 0\ntmpfs /tmp tmpfs rw 0 0\n",
    );
    assert_eq!(
        removable_mounts(&r),
        vec!["/mnt/sd".to_string(), "/mnt/usb/sda1".to_string()]
    );
    assert!(removable_mounts(&Root::new(tmp())).is_empty());
}
