## ADDED Requirements

### Requirement: Install
The system SHALL install a catalogue entry with apk from the configured signed repositories and SHALL add its apps.json entry only after apk succeeds.

#### Scenario: the user installs Mousepad
- **WHEN** the user installs Mousepad
- **THEN** the package is installed and a Mousepad icon appears on Home

#### Scenario: apk fails
- **WHEN** apk fails
- **THEN** no apps.json entry is added and the error is shown

### Requirement: Uninstall
The system SHALL remove the package and its apps.json entry and close the app if it runs.

#### Scenario: Mousepad is uninstalled while running
- **WHEN** Mousepad is uninstalled while running
- **THEN** the window closes, the package is removed and the icon disappears

### Requirement: Space check
The system SHALL refuse an install when free space would drop below 2 GiB.

#### Scenario: an install needs 3 GiB with 4 GiB free
- **WHEN** an install needs 3 GiB with 4 GiB free
- **THEN** the screen explains that space is insufficient and nothing changes

### Requirement: Default apps
The system SHALL record default apps per role and use them when opening a file from Files or xdg-open.

#### Scenario: the default PDF viewer is changed
- **WHEN** the default PDF viewer is changed
- **THEN** opening a PDF starts the chosen viewer

### Requirement: Untrusted packages
The system SHALL NOT install packages with --allow-untrusted from the UI.

#### Scenario: a local .apk without a trusted signature is selected
- **WHEN** a local .apk without a trusted signature is selected
- **THEN** the install is refused
