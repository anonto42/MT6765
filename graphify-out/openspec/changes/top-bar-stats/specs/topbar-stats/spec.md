## ADDED Requirements

### Requirement: Live figures in the header
The shell SHALL show CPU use, RAM used of total, storage used of the userdata root and network down/up rate in the header row, each only while its switch is on, and SHALL show an unavailable figure as "--".

#### Scenario: defaults
- **WHEN** the tablet starts with no stored choice
- **THEN** CPU and RAM are shown and storage and network are not

#### Scenario: small screen
- **WHEN** the enabled items do not fit beside the clock and the Wi-Fi/battery pill
- **THEN** the date is dropped first, then the bars, and nothing overlaps

### Requirement: Switches
Settings > Top bar items SHALL have one switch per item; the core SHALL persist them in /var/lib/walpad/statusbar.json and SHALL not sample while all are off.

#### Scenario: all switches off
- **WHEN** the owner turns all four off
- **THEN** the header shows no stats and the core reads no /proc statistics
