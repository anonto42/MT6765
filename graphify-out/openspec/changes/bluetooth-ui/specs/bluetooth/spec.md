## ADDED Requirements

### Requirement: Power and scan
The system SHALL power the controller on and list nearby devices with name and signal strength.

#### Scenario: Bluetooth is switched on
- **WHEN** Bluetooth is switched on
- **THEN** hci0 is powered and nearby devices appear within 15 s

### Requirement: Pairing
The shell SHALL show a confirmation sheet with the passkey and complete pairing when the user confirms.

#### Scenario: a phone starts pairing
- **WHEN** a phone starts pairing
- **THEN** the sheet shows the 6 digit code and Pair completes the bond

### Requirement: Wi-Fi isolation
Starting or stopping Bluetooth SHALL NOT disconnect Wi-Fi and SHALL fail safe within 30 s.

#### Scenario: Bluetooth is toggled 5 times
- **WHEN** Bluetooth is toggled 5 times
- **THEN** Wi-Fi stays connected and no process hangs

### Requirement: File receive
The system SHALL accept an OBEX push after user confirmation and store the file in ~/Downloads.

#### Scenario: a phone sends a photo
- **WHEN** a phone sends a photo
- **THEN** a confirmation is shown and the file is saved after Accept

### Requirement: Honest capability
Unsupported profiles (keyboard, audio) SHALL be shown as unavailable rather than hidden until their modules exist.

#### Scenario: a Bluetooth keyboard is paired without hidp.ko
- **WHEN** a Bluetooth keyboard is paired without hidp.ko
- **THEN** the device shows Paired, keyboard unavailable
