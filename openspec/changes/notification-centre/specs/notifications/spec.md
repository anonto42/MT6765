## ADDED Requirements

### Requirement: Notification service
The shell SHALL own the D-Bus name org.freedesktop.Notifications on the session bus and answer Notify with a stable id.

#### Scenario: `notify-send Hi body` runs
- **WHEN** `notify-send Hi body` runs
- **THEN** a banner appears within 500 ms and an entry is added to the shade

### Requirement: Shade
The shell SHALL list active notifications grouped by app in a shade opened from the header, allow dismissing one or all, and invoke the notification's default action on tap.

#### Scenario: the user taps a notification with a default action
- **WHEN** the user taps a notification with a default action
- **THEN** ActionInvoked is emitted and the entry is removed

### Requirement: Do Not Disturb
While Do Not Disturb is on, the shell SHALL NOT show heads-up banners except for urgency critical, and SHALL still record the notifications in the shade.

#### Scenario: DND is on and a normal notification arrives
- **WHEN** DND is on and a normal notification arrives
- **THEN** no banner is shown and the shade has the entry

### Requirement: Bounds
The shell SHALL keep at most 100 notifications and drop the oldest when the limit is exceeded; bodies SHALL be truncated to 2000 bytes and markup stripped.

#### Scenario: 200 notifications are sent in a loop
- **WHEN** 200 notifications are sent in a loop
- **THEN** the shade shows 100 and the shell stays responsive
