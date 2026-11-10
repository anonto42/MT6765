## 1. Probe (read-only)
- [ ] 1.1 Device nodes, `/proc/driver/camera_info`, dmesg imgsensor/pvr lines, binder nodes

## 2. Factory ACDK test (owner OK needed: possible nvdata writes, display takeover)
- [ ] 2.1 Run `/vendor/bin/factory` via linker64 + libwpdprops.so with build props loaded
- [ ] 2.2 Main and sub camera preview; capture `/data/acdkCap.jpg`; copy to host and look

## 3. ACDK helper + camera app (if 2 works)
- [ ] 3.1 Recover ACDK_CMD_* numbers and ACDK_PREVIEW_STRUCT from factory's call sites
- [ ] 3.2 `walpad-camera` bionic helper: preview frames to /run/walpad, JPEG capture
- [ ] 3.3 Rust camera app (apps.json id `camera`), host checks pass

## 4. Fallbacks
- [ ] 4.1 `uvcvideo.ko` for USB webcams (same build path as hci_vhci.ko)
- [ ] 4.2 Halium-style HAL experiment (hwservicemanager, gralloc, camerahalserver, lshal)
