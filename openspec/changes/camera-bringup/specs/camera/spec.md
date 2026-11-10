## ADDED Requirements

### Requirement: Still photo from the built-in camera
The system SHALL capture a JPEG photo from the main camera without the Android camera HAL, using
the vendor ACDK libraries, and save it where the user's files are visible.

#### Scenario: Capture from the camera app
- **WHEN** the user opens Camera and taps the shutter
- **THEN** a JPEG from the main sensor is saved under `/root/Pictures` within 3 seconds and the
  app shows it

#### Scenario: Camera unavailable
- **WHEN** the ACDK path fails to open the sensor
- **THEN** the app says "Camera unavailable" with the reason from the helper log and nothing else
  on the device is affected
