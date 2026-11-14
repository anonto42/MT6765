## ADDED Requirements

### Requirement: Layout data
The keyboard SHALL load layouts from JSON files and SHALL fall back to en-us when a file is invalid.

#### Scenario: a malformed bn file is installed
- **WHEN** a malformed bn file is installed
- **THEN** the keyboard shows en-us and a toast names the broken file

### Requirement: Language switching
The keyboard SHALL provide a language key that cycles the enabled layouts and shows the current one.

#### Scenario: the key is tapped with en-us and bn-phonetic enabled
- **WHEN** the key is tapped with en-us and bn-phonetic enabled
- **THEN** bn-phonetic is active and the label shows it

### Requirement: Unicode commit
The shell SHALL commit non-Latin text to the focused application through the XIM server.

#### Scenario: bn-phonetic is active and the user types `ami`
- **WHEN** bn-phonetic is active and the user types `ami`
- **THEN** Firefox's input contains আমি

### Requirement: Emoji
The keyboard SHALL offer an emoji layer with categories and a recents row and SHALL insert the chosen emoji as text.

#### Scenario: an emoji is tapped
- **WHEN** an emoji is tapped
- **THEN** the character is inserted and appears first in recents

### Requirement: Fonts
The image SHALL contain a Bengali-capable font so typed Bangla renders in Firefox and the terminal.

#### Scenario: Bangla text is shown in Firefox
- **WHEN** Bangla text is shown in Firefox
- **THEN** no tofu boxes are rendered
