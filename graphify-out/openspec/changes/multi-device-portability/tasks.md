# Tasks

## 1. Hardware Abstraction Traits and Capability Model

- [x] 1.1 Implement `Capability` enum and HAL backend traits (`DisplayBackend`, `BatteryBackend`, `InputBackend`, `AudioBackend`, `NetworkBackend`, `PowerBackend`) in `crates/walpad-hw/src/domain/capability.rs` and verify with unit tests
- [x] 1.2 Implement `DeviceProfile` domain parser and fallback defaults (`default_walpad_8g`, `generic_arm64`) in `crates/walpad-hw/src/domain/device_profile.rs` and verify with unit tests in `crates/walpad-hw/src/domain/device_profile/tests.rs`

## 2. Platforms and Profiles Structure

- [x] 2.1 Create Walton WalPad 8G reference configuration in `platforms/walton/walpad-8g/` (`device.toml`, `hardware.toml`, `display.toml`, `input.toml`, `network.toml`, `power.toml`, `boot/`, `firmware/`) and verify TOML structure
- [x] 2.2 Create generic arm64 fallback profile in `platforms/arm64-generic/device.toml` and reference tablet templates in `platforms/samsung/example-tablet/device.toml` and `platforms/lenovo/example-tablet/device.toml`

## 3. Device Detector Binary (`walpad-device-detect`)

- [x] 3.1 Create `crates/walpad-device-detect` crate with hardware probing (`probe.rs`) for devicetree, cpuinfo, display, power, evdev inputs, and network
- [x] 3.2 Implement CLI interface in `crates/walpad-device-detect/src/main.rs` supporting formatted human-readable report, `--json` structured audit, `--version`, and profile matching
- [x] 3.3 Register `crates/walpad-device-detect` in root `Cargo.toml` and `scripts/build_rust.sh` and verify compilation and unit tests

## 4. Image Bundling Specifications and Documentation

- [x] 4.1 Create `images/generic-rootfs/README.md` and `images/device-bundles/walton-walpad-8g/bundle.toml` specifying OS base rootfs and device-specific bundle layout
- [x] 4.2 Create `tools/image-builder/README.md` documenting image assembly and bundling architecture
- [x] 4.3 Write multi-device documentation in `docs/plans/50-multi-device-portability.md` and `docs/plans/51-device-porting-guide.md` and update `docs/rust/CONTRACTS.md`


## 5. Verification and Integration

- [x] 5.1 Run `scripts/check.sh` and verify all tests, clippy, cross-compilation, qemu smoke test, and shell checks pass

