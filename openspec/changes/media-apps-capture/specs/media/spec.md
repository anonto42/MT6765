## ADDED Requirements

### Requirement: Video playback
The system SHALL play H.264 up to 720p30 with sound through the single 44.1 kHz audio path without hanging the tablet.

#### Scenario: a 720p30 MP4 plays for 5 minutes
- **WHEN** a 720p30 MP4 plays for 5 minutes
- **THEN** audio stays in sync and no more than 5 % of frames are dropped

### Requirement: Document and image viewing
The system SHALL open PDF and common image files from Files in the default viewers.

#### Scenario: a PDF is opened
- **WHEN** a PDF is opened
- **THEN** the viewer shows page 1 within 5 s and scrolls by dragging

### Requirement: Screenshot
The shell SHALL save the visible screen as a PNG under ~/Pictures/Screenshots and post a notification.

#### Scenario: the screenshot action runs
- **WHEN** the screenshot action runs
- **THEN** a PNG of the current screen exists and a notification with its name appears

### Requirement: Screen recording
The system SHALL record the screen to an MP4 at no more than 15 fps and stop automatically when free RAM falls below 15 %.

#### Scenario: recording is started then stopped
- **WHEN** recording is started then stopped
- **THEN** an MP4 plays and its frame rate is at most 15

### Requirement: Audio safety
Media apps SHALL output only via the ALSA PCM configured for 44.1 kHz.

#### Scenario: a 48 kHz file is played
- **WHEN** a 48 kHz file is played
- **THEN** it is resampled and the tablet does not hang
