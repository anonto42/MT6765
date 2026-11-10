## ADDED Requirements

### Requirement: Alarms
The core SHALL fire an alarm at its scheduled local time with the screen on or off and SHALL persist alarms across restarts.

#### Scenario: an alarm is set 2 minutes ahead and the screen is off
- **WHEN** an alarm is set 2 minutes ahead and the screen is off
- **THEN** the screen wakes, the alarm sound plays and a notification with Snooze and Dismiss is shown

#### Scenario: the tablet reboots before the time
- **WHEN** the tablet reboots before the time
- **THEN** the alarm still fires

### Requirement: Timer and stopwatch
The shell SHALL provide a countdown timer and a stopwatch that keep time while another app is in front.

#### Scenario: a 1 minute timer is started and the user opens Firefox
- **WHEN** a 1 minute timer is started and the user opens Firefox
- **THEN** the timer fires after 60 seconds

### Requirement: Calculator
The shell SHALL provide a calculator with standard and scientific modes and exact decimal display for basic operations.

#### Scenario: 12*(3+4) is entered
- **WHEN** 12*(3+4) is entered
- **THEN** 84 is shown

### Requirement: Notes
The shell SHALL store notes as plain Markdown files under ~/Notes and list, create, edit and delete them.

#### Scenario: a note is created and the tablet reboots
- **WHEN** a note is created and the tablet reboots
- **THEN** the note is listed with the same content

### Requirement: Time change
Alarms SHALL follow the system timezone and SHALL NOT fire twice after an NTP clock step.

#### Scenario: the clock is stepped forward 1 minute
- **WHEN** the clock is stepped forward 1 minute
- **THEN** an alarm fires once
