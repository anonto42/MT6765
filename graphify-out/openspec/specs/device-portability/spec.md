# device-portability Specification

## Purpose
Enables WalPad OS to execute across diverse tablet and embedded hardware platforms through modular device profiles, hardware abstraction traits, hardware auto-detection, and tier compatibility classification.

## Requirements

### Requirement: Structured device profile schema
The system SHALL provide a structured device profile schema defining device metadata, display dimensions, input mappings, network controllers, power interfaces, audio parameters, and hardware capabilities.

#### Scenario: Parse valid device profile
- **WHEN** a valid `device.toml` file is loaded by the system
- **THEN** the device identifier, SoC model, compatibility tier, display dimensions, input patterns, and capabilities are parsed into a strongly-typed profile structure without panicking

#### Scenario: Fallback on missing or invalid profile
- **WHEN** a profile file is absent, unreadable, or invalid
- **THEN** the system SHALL fall back to a safe default profile (`default_walpad_8g` or `generic_arm64`) with status indicating fallback mode

### Requirement: Hardware capability reporting
The system SHALL classify hardware capabilities as Supported, Unsupported, or Unknown, and SHALL NOT assume standard interfaces exist across all target platforms.

#### Scenario: Sensed capability is absent
- **WHEN** a subsystem queries a device capability (such as cellular modem or camera) that is not supported or present on the platform
- **THEN** the query returns `Capability::Unsupported` or `None` and does not crash or interrupt OS services

### Requirement: Hardware auto-detection
The system SHALL provide a device detection utility (`walpad-device-detect`) that discovers system CPU architecture, devicetree compatibility, input devices, display backlight, power supplies, network interfaces, and audio cards.

#### Scenario: Human-readable hardware audit
- **WHEN** `walpad-device-detect` is executed with default arguments
- **THEN** it inspects system nodes, matches known profiles, and outputs a formatted hardware report displaying architecture, detected components, and the assigned compatibility tier

#### Scenario: Machine-readable JSON output
- **WHEN** `walpad-device-detect` is executed with `--json`
- **THEN** it outputs a valid JSON document containing system architecture, SoC strings, detected peripherals, matched profile, and capability status
