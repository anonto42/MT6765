# Multi-device portability architecture

## Why

Owner request 2026-11-15: "Making WalPad OS compatible with other tablets and devices. Think about the full implementations and implement this."
Currently, WalPad OS has hardware paths and assumptions tailored exclusively to the Walton WalPad 8G (MT6765). To make WalPad OS a reusable, multi-device embedded Linux operating system, we decouple OS components into a 4-layer architecture: WalPad Shell (device-agnostic UI) -> WalPad Core (reusable services) -> Hardware Abstraction Layer (HAL traits) -> Device Platform (device-specific bundles).

## What changes

- Hardware Abstraction Layer (`crates/walpad-hw`):
  - Trait definitions: `DisplayBackend`, `BatteryBackend`, `InputBackend`, `AudioBackend`, `NetworkBackend`, `PowerBackend`, and `Capability` enum (`Supported`, `Unsupported`, `Unknown`).
  - `DeviceProfile` domain model: loads structured device profiles (`device.toml`), queries capabilities, and determines system roles without hardcoding sysfs paths.
  - Builtin profiles and automatic fallback: `DeviceProfile::default_walpad_8g()` and `DeviceProfile::generic_arm64()`.
- Device Detector Binary (`crates/walpad-device-detect`):
  - New binary crate in the workspace.
  - Probes system hardware (devicetree compatible, cpuinfo, display/backlight, power_supply, input evdev, network, audio, sensors).
  - Matches probed identifiers against known device profiles.
  - Outputs human-readable diagnostic report with Tier 1/2/3 compatibility classification, or `--json` structured audit.
- Repository Device Profiles (`platforms/`):
  - `platforms/walton/walpad-8g/`: Reference device (Tier 1) configuration files (`device.toml`, `hardware.toml`, `display.toml`, `input.toml`, `network.toml`, `power.toml`, `boot/`, `firmware/`).
  - `platforms/arm64-generic/`: Fallback profile (Tier 3) for generic aarch64 platforms (`device.toml`).
  - `platforms/samsung/example-tablet/`: Placeholder reference profile for Exynos/Qualcomm hardware (`device.toml`).
  - `platforms/lenovo/example-tablet/`: Placeholder reference profile for Unisoc/MediaTek hardware (`device.toml`).
- Device Bundle and Base Image Architecture:
  - `images/generic-rootfs/README.md`: Specifications for the device-agnostic Alpine Linux ARM64 rootfs base image.
  - `images/device-bundles/walton-walpad-8g/bundle.toml`: Reference device bundle definition.
  - `tools/image-builder/README.md`: Architectural tooling specification for assembling base rootfs + device bundle.
- Documentation:
  - `docs/plans/50-multi-device-portability.md`: Architecture guide, 4-layer design, profile schema, tier definitions.
  - `docs/plans/51-device-porting-guide.md`: Step-by-step workflow for porting new tablets/devices.
  - `docs/rust/CONTRACTS.md`: Profile contract and capability discovery documentation.

## Capabilities

### New Capabilities
- `device-portability`: Device profile definition, hardware abstraction traits, hardware auto-detection, and Tier classification.

### Modified Capabilities
- (None)

## Out of scope

- Physical flashing or partition resizing on any device (no flashing tools run).
- Actual bring-up of secondary hardware (Samsung/Lenovo directories provide schema reference profiles only).
- Refactoring frozen components (`config/boot/*`, `tools/propshim/`, `walpad-wifi` CONNSYS section).

## Device risk

None. All probe code is read-only sysfs/proc reads with strict timeouts and error fallbacks. No block devices or partitions are written.

## Secrets impact

None. No credentials, tokens, or private identifiers are stored or logged.

## Verification

- `scripts/check.sh` passes all format, clippy, unit, snapshot, cross-compile, and shell tests.
- `walpad-device-detect` builds for aarch64 and executes `--version` and `--json`.
- Unit tests verify profile parsing, trait implementations, fallback logic, and device detection scoring.
