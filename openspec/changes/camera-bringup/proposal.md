# Camera bring-up on WalPad OS

## Why

Owner request 2026-11-10: "for camera find all the possible ways to solve this and work on this OS".
The kernel has the MediaTek camera drivers (kd_camera_hw, camera-isp, ccu, seninf) but no V4L2
capture driver; the stock camera stack is the Android HAL.

## What changes

Ordered experiments, each gated on the previous result:

1. Read-only probe of the camera device nodes, sensors and binder/GPU nodes.
2. Run MediaTek's factory camera test (`/vendor/bin/factory`, ACDK via `libacdk.so`) through the
   64-bit bionic linker with the property shim: preview on fb0 and `/data/acdkCap.jpg`.
3. If 2 works: `walpad-camera`, a small bionic helper on `libacdk` (MDK_Open/Init/IOControl) that
   streams preview frames to `/run/walpad` and captures JPEGs, plus a Rust camera app.
4. Fallbacks: USB webcam via an out-of-tree `uvcvideo.ko`; Halium-style camera HAL
   (hwservicemanager + camerahalserver + HIDL client) for full 3A/video.

Sensors from the kernel config: gc5035, gc8034, gc08a3, gc2385, sc200cs.

## Out of scope

Android's init or any SELinux policy load (cannot be made permissive on this kernel); the GPU
user space unless the HAL route needs it.

## Device risk

No flashing. The camera libraries may write default calibration files into `nvdata` (holds the
IMEI): NV backups exist in `firmware/MT6765/backups/modem-20261110/`. The factory binary takes the
framebuffer and keys while it runs (stop fbmirror first, restart the session after). A crash
kills only the process.

## Verification

`/proc/driver/camera_info` names the sensors; `/data/acdkCap.jpg` is a real photo; later the
camera app shows a live preview on the tablet.
