# WalPad OS Generic Base Image (`generic-rootfs`)

## Overview
The WalPad OS Generic Base Image is a device-agnostic ARM64 Linux root filesystem built on Alpine Linux. It contains all common OS services, utilities, and applications, and has **zero hardware-specific paths compiled in**.

## Contents of Base Image
1. **Alpine Linux ARM64 Base**:
   - Musl libc runtime environment
   - Minimal core utilities (busybox/coreutils)
   - OpenRC init and service configuration

2. **WalPad Rust Core & Graphical Shell**:
   - `walpad-core`: Central system daemon managing IPC, state, and power
   - `walpad-shell`: Graphical touch interface running on tiny-skia
   - `walpad-ctl`: Command-line management utility
   - `walpad-device-detect`: Hardware detection and profile matching binary
   - `walpad-btbridge` & `walpad-gnss`: Portable hardware bridge daemons

3. **Display Server Stack**:
   - `Xvfb` (headless X11 server for frame rendering)
   - `openbox` window manager
   - `fbmirror` framebuffer blitter

4. **Applications**:
   - Web browser (Firefox ESR)
   - Terminal emulator (`st`)
   - File manager (`pcmanfm`)

5. **Audio & Media**:
   - PulseAudio sound server and ALSA client library

## Hardware Portability Contract
The generic rootfs expects a hardware profile to be mounted or provisioned at `/etc/walpad/device.toml`. If omitted, `walpad-core` automatically triggers `walpad-device-detect` to probe available hardware nodes and apply safe fallback capabilities.
