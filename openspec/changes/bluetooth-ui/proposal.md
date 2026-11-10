# Bluetooth settings

## Why

Owner request 2026-11-10. BlueZ, hci_vhci.ko and walpad-btbridge exist (docs/30-bluetooth.md) but the Quick Settings Bluetooth row says Not supported and there is no pairing UI.

## What changes

- Device test of the existing chain is the first task; start `walpad-bluetooth` from walpad-init only after it passes.
- Core talks to bluetoothd over D-Bus (zbus): power, scan, pair, connect, forget, rename; status.bluetooth in status.json.
- Shell: Quick Settings tile and Settings > Bluetooth screen (paired, available, passkey confirmation sheet).
- File receive with bluez-obexd into ~/Downloads with a notification.

## Out of scope

A2DP audio (BlueALSA and the audio quirk), HID keyboards (needs hidp.ko), RFCOMM/serial, PAN. Each is a follow-up once the module batch exists.

## Device risk

Loads hci_vhci.ko and runs the btbridge: a wrong stpbt sequence could hang the connectivity chip shared with Wi-Fi: every call has a timeout, Wi-Fi is checked after Bluetooth start. No flashing.

## Verification

On the tablet: `walpad-bluetooth start`, `bluetoothctl show` lists hci0, scan finds the owner's phone, pair, send a file to the tablet; Wi-Fi stays connected throughout.

## Open questions

- Does /dev/stpbt stay stable across repeated open/close? Unknown until the device test.
