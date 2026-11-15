# Design: Multi-Device Portability Architecture

## Context

See `proposal.md` for background and motivation. WalPad OS currently accesses hardware through `crates/walpad-hw` with a `Root` prefix (`/sys/...`). While `Root` allows test isolation against temporary directory trees, path strings (backlight sysfs, battery power_supply, input device names) are compiled-in constants. Target devices across different manufacturers (Walton, Samsung, Lenovo) differ in SoC, display resolution, touchscreen drivers, and power topologies.

## Goals / Non-Goals

**Goals:**
- Provide a clear 4-layer architecture: WalPad Shell -> WalPad Core -> HAL -> Device Platform.
- Define a device profile schema (`device.toml`, `hardware.toml`, etc.) in `platforms/` capturing hardware-specific configurations.
- Introduce HAL traits in `crates/walpad-hw` (`DisplayBackend`, `BatteryBackend`, `InputBackend`, etc.) and a `Capability` enum.
- Create `crates/walpad-device-detect` to probe hardware, match known profiles, and report compatibility tiers.
- Separate generic OS rootfs base specifications from device bundles.
- Keep Walton WalPad 8G fully functional as the Tier 1 reference platform with zero regression.

**Non-Goals:**
- Actual physical flashing of secondary hardware (Samsung and Lenovo configurations serve as profile templates).
- Modifying locked bootloader components or frozen MTK scripts (`config/boot/*`, `tools/propshim/`, `walpad-wifi`).

## Decisions

### Decision 1: Pure Rust Lightweight Profile Parser with serde
- *Rationale*: We support standard TOML profiles in `platforms/` using a zero-dependency, safe parser integrated into `walpad-hw`, backed by `serde`/`serde_json` structures already present in the workspace.
- *Alternatives considered*: Adding external `toml` crate (which pulls in multiple transitive dependencies into musl cross-compile). A lightweight parser in `walpad-hw` guarantees reproducible, offline builds with zero C dependencies.

### Decision 2: Fallback Profiles and Safe Defaults
- *Rationale*: If no device profile matches devicetree or CPU info, the system automatically falls back to `arm64-generic` (or `walton-walpad-8g` if running on MT6765). Missing sensors or hardware interfaces return `Option::None` or `Capability::Unsupported`.
- *Alternatives considered*: Failing boot if profile is missing. Rejected because an embedded OS must boot into rescue or generic shell even if identification fails.

### Decision 3: Three-Tier Compatibility Classification
- *Tier 1 (Fully Supported)*: Complete hardware certification, display, touch, buttons, Wi-Fi, audio, battery tested. (Walton WalPad 8G).
- *Tier 2 (Basic Operation)*: Boots Linux and runs UI, but may have known omissions (e.g. modem, camera, hardware video acceleration).
- *Tier 3 (Experimental)*: Boots generic kernel and framebuffer; basic touch/evdev input, uncalibrated power/audio.

## Risks / Trade-offs

- [Risk] Divergent sysfs naming conventions across kernel versions.
  -> *Mitigation*: Probe routines in `walpad-device-detect` scan glob patterns (`/sys/class/leds/*backlight*`, `/sys/class/power_supply/*`) rather than hardcoding exact node paths.
- [Risk] Performance impact of profile loading at startup.
  -> *Mitigation*: Device profile is loaded once at daemon startup, cached in memory, and passed via reference.

## Migration Plan

1. Implement HAL traits and capability enums in `crates/walpad-hw`.
2. Implement `DeviceProfile` domain model with built-in reference profiles in `crates/walpad-hw`.
3. Create `platforms/` directory tree with Walton WalPad 8G (Tier 1), arm64-generic (Tier 3), and templates.
4. Implement `crates/walpad-device-detect` binary crate.
5. Create specifications in `images/` and `tools/image-builder/`.
6. Add unit tests and verify `scripts/check.sh` passes.
