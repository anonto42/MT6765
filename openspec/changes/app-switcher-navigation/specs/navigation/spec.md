## ADDED Requirements

### Requirement: Recents overview
The shell SHALL show a Recents strip with one card per running app (thumbnail, name) when the user swipes up from the bottom edge and holds.

#### Scenario: the user swipes up from the bottom edge and holds 300 ms wit
- **WHEN** the user swipes up from the bottom edge and holds 300 ms with three apps running
- **THEN** three cards are shown within 400 ms, most recent first

#### Scenario: the user taps a card
- **WHEN** the user taps a card
- **THEN** that app is focused and the strip closes

### Requirement: Close from Recents
The shell SHALL close an app when its card is swiped away, by sending the existing `close_app` command for that app id.

#### Scenario: a card is swiped up off the strip
- **WHEN** a card is swiped up off the strip
- **THEN** the app process exits and the card disappears

#### Scenario: the core does not answer
- **WHEN** the core does not answer
- **THEN** the card returns and a toast says the app could not be closed

### Requirement: Home and Back gestures
The shell SHALL treat a short bottom-edge swipe as Home and the Back control as a key event (Alt+Left, else Esc) to the focused window.

#### Scenario: a short swipe up from the bottom edge
- **WHEN** a short swipe up from the bottom edge
- **THEN** the Home screen is shown and the running apps keep running

#### Scenario: Back is pressed in Files
- **WHEN** Back is pressed in Files
- **THEN** PCManFM navigates to the previous folder

### Requirement: No interference with apps
Edge gestures SHALL start only in a 24 px strip at the screen bottom outside the keyboard area, and SHALL NOT consume touches that start elsewhere.

#### Scenario: the user drags inside Firefox starting 100 px above the bott
- **WHEN** the user drags inside Firefox starting 100 px above the bottom
- **THEN** the page scrolls and no gesture fires
