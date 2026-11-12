## ADDED Requirements

### Requirement: Headphone output route
The system SHALL let the owner switch audio output between the speaker and the headphone path
through a manual override, and SHALL report the route honestly (never claiming a live sensor
reading the current kernel build cannot provide).

#### Scenario: owner forces headphone output
- **WHEN** the owner sends `jack_force {route: "headphones"}` (or `walpad-ctl jack headphones`)
- **THEN** the speaker amp switches turn off, the headset amp switch turns on, and
  `status.jack.route` reports `"headphones"`

#### Scenario: owner clears the override
- **WHEN** the owner sends `jack_force {route: null}` (or `walpad-ctl jack off`)
- **THEN** the speaker amp switches turn back on and `status.jack.route` reports `null`

#### Scenario: no live jack-insert detection
- **WHEN** nothing has loaded the kernel's accdet module
- **THEN** `status.jack.detected` stays `null`; it is never guessed from the override

### Requirement: Microphone capture path
The system SHALL expose a microphone capture path (ALSA capture PCM + PulseAudio source) that can
be pointed at the built-in mic or a headset mic through the same manual override, using only
mixer controls confirmed to exist on the device.

#### Scenario: built-in mic selected
- **WHEN** no override is in effect (or the override is `"headphones"` with no confirmed mic)
- **THEN** the capture mux is pointed at the built-in mic chain (ADC1)

#### Scenario: headset mic selected
- **WHEN** the override is `"headset_with_mic"`
- **THEN** the capture mux is pointed at the headset mic chain (ADC2)
