# Firmware Requirements: Walton WalPad 8G

## Required Firmware Blobs
- Wi-Fi & Bluetooth: MediaTek CONNSYS ROM and patch files loaded by `wmt_loader` via vendor firmware partitions (`/vendor/firmware/`).
- Cellular Modem: MD1 baseband ROM and DSP images mounted on `/vendor/firmware/` and loaded by `ccci` driver.
- Touchscreen: Silead GSL3670 firmware blob mapped via kernel driver `gslx680`.
- Display: MIPI DSI display driver IC initialization sequence contained in MTK display driver.
