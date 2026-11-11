## ADDED Requirements

### Requirement: Calm battery percent
Without a working fuel gauge the system SHALL show a battery percent that never rises while discharging, never falls while charging, changes by at most one point per minute, and shows 100 only when the battery reports Full.

#### Scenario: brightness is raised
- **WHEN** brightness or CPU load causes the battery voltage to sag
- **THEN** the shown percent does not rise and drops by at most one point per minute

#### Scenario: reboot
- **WHEN** the tablet boots and the saved percent agrees with the rested voltage
- **THEN** the percent resumes from the saved value

### Requirement: Time left
The system SHALL estimate time left while discharging and time to full while charging from the smoothed battery current and the capacity, label it as an estimate, and show nothing when it is unknown.

#### Scenario: first minutes
- **WHEN** less than two minutes of current data exist
- **THEN** the UI shows "calculating..."
