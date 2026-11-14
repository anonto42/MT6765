## ADDED Requirements

### Requirement: App drawer and search
The shell SHALL list all apps alphabetically and filter apps and settings entries as the user types.

#### Scenario: `wifi` is typed in search
- **WHEN** `wifi` is typed in search
- **THEN** the Network settings entry is listed and opens that page

### Requirement: Folders
The shell SHALL let the user group apps into folders and persist the layout.

#### Scenario: two apps are dropped onto each other
- **WHEN** two apps are dropped onto each other
- **THEN** a folder with both appears and survives a reboot

### Requirement: Widgets
The shell SHALL draw native widgets on the Home page and update them at most once per minute without extra processes.

#### Scenario: the clock widget is added
- **WHEN** the clock widget is added
- **THEN** it shows the time and CPU use stays idle

### Requirement: Layout safety
A corrupt or missing launcher.json SHALL fall back to the default grid.

#### Scenario: launcher.json is truncated
- **WHEN** launcher.json is truncated
- **THEN** Home shows every app in a plain grid and a toast mentions the reset

### Requirement: Orientation
Home layout SHALL adapt to portrait and landscape without losing folders.

#### Scenario: the tablet rotates
- **WHEN** the tablet rotates
- **THEN** icons reflow and folders keep their contents
