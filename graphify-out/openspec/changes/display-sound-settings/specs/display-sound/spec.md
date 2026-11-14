## ADDED Requirements

### Requirement: Wallpaper
The shell SHALL let the user choose a wallpaper from the shipped set and ~/Pictures and SHALL persist the choice in shell.json.

#### Scenario: a picture is chosen
- **WHEN** a picture is chosen
- **THEN** Home, Settings and Quick Settings use it as background immediately and after a reboot

### Requirement: Text size
The shell SHALL scale all text and row heights by a user-chosen factor (0.9, 1.0, 1.2) while keeping touch targets at least 56 px.

#### Scenario: Large is chosen
- **WHEN** Large is chosen
- **THEN** no text is clipped on Settings, Network and Mobile screens at 800 px width

### Requirement: Volume channels
The system SHALL provide Media, Alarm and Notification volume controls, all routed through the single 44.1 kHz ALSA mixer configuration.

#### Scenario: the Media slider is set to 30 %
- **WHEN** the Media slider is set to 30 %
- **THEN** media playback is quieter and the notification volume is unchanged

### Requirement: Audio safety
The system SHALL NOT open any ALSA device with a rate other than 44.1 kHz or buffer settings other than 25 ms period and 100 ms buffer.

#### Scenario: two sounds play at once
- **WHEN** two sounds play at once
- **THEN** both are mixed by dmix and the tablet stays responsive for 60 s

### Requirement: Night light
The shell SHALL apply a warm tint from a configurable start time until sunrise time and SHALL show the setting in Quick Settings.

#### Scenario: the schedule starts
- **WHEN** the schedule starts
- **THEN** the screen shows a warm tint and the tile is on
