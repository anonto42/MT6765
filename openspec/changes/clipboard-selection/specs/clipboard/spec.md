## ADDED Requirements

### Requirement: Clipboard persistence
The shell SHALL keep the last CLIPBOARD text available to paste after the application that provided it has exited.

#### Scenario: text is copied in Firefox and Firefox is closed
- **WHEN** text is copied in Firefox and Firefox is closed
- **THEN** pasting in the terminal inserts the same text

### Requirement: History
The shell SHALL keep the last 25 distinct text items in memory, offer them in a history sheet, and let the user pin, paste or clear them.

#### Scenario: 30 different items are copied
- **WHEN** 30 different items are copied
- **THEN** only the newest 25 remain, pinned items stay

#### Scenario: the user taps Clear
- **WHEN** the user taps Clear
- **THEN** the history is empty and the current clipboard is emptied

### Requirement: Touch edit popup
The shell SHALL show Cut, Copy, Paste and Select all when the user long-presses (600 ms) in a focused text input and SHALL send the matching key chord to the focused window.

#### Scenario: the user long-presses in the Firefox URL bar and taps Select
- **WHEN** the user long-presses in the Firefox URL bar and taps Select all then Copy
- **THEN** the URL text is on the clipboard

### Requirement: No persistence of secrets
The clipboard history SHALL NOT be written to disk and SHALL NOT appear in logs, status.json or command arguments.

#### Scenario: the tablet reboots
- **WHEN** the tablet reboots
- **THEN** the history is empty

#### Scenario: log files are searched for a copied string
- **WHEN** log files are searched for a copied string
- **THEN** no match is found
