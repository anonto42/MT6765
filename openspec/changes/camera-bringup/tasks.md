## 1. Probe (read-only)
- [ ] 1.1 Device nodes, `/proc/driver/camera_info`, dmesg imgsensor/pvr lines, binder nodes (device test plan step 1, docs/31-camera-acdk.md)

## 2. Factory ACDK test (owner OK needed: possible nvdata writes, display takeover)
- [x] 2.1 `/vendor/bin/factory` runs through linker64 + libwpdprops.so (verified 2026-11-10 with VNDK apex, binderfs + vndservicemanager, selinuxfs, fake atag,boot = 4); its GUI garbles the panel, so it is only a reference
- [ ] 2.2 (superseded by 4.x: the helper replaces the factory test)

## 3. ACDK helper + camera app
- [x] 3.1 Recover ACDK_CMD_* numbers and ACDK_PREVIEW_STRUCT / ACDK_CAPTURE_STRUCT from factory's call sites (docs/31-camera-acdk.md, with addresses and confidence)
- [x] 3.2 `camacdk` (tools/camacdk, bionic aarch64, freestanding) and `walpad-camera` (start|stop|capture|preview|list|status), image install, host tests in tests/shell/run.sh
- [ ] 3.3 Rust camera app (apps.json id `camera`), host checks pass; shutter calls `walpad-camera capture`, saves under /root/Pictures

## 4. Device bring-up of the helper (docs/31-camera-acdk.md section 9)
- [ ] 4.1 `walpad-camera start`: binderfs, vndservicemanager, selinuxfs, VNDK apex, props
- [ ] 4.2 `walpad-camera list`: libacdk loads, main and/or sub sensor opens (`MDK_OpenBypass(4)` fake surface view)
- [ ] 4.3 `walpad-camera capture main /root/x.jpg`, copy to the host and look at it; then `sub`
- [ ] 4.4 `walpad-camera preview main /root/prv 5` (least certain: relies on the libacdk preview dump)
- [ ] 4.5 Fix what the device shows (crt entry point, `CAMACDK_BYPASS=0`, NVRAM/calibration files, missing /dev nodes); update docs/31 section 8

## 5. Fallbacks
- [ ] 5.1 `uvcvideo.ko` for USB webcams (same build path as hci_vhci.ko)
- [ ] 5.2 Halium-style HAL experiment (hwservicemanager, gralloc, camerahalserver, lshal)
