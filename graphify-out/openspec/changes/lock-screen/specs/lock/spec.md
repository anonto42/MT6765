## ADDED Requirements

### Requirement: Setting a lock
The core SHALL store only a salted hash of the PIN or password and SHALL never log, echo or place the secret in argv.

#### Scenario: the user sets PIN 4321
- **WHEN** the user sets PIN 4321
- **THEN** lock.json has mode 0600 and contains no plaintext digits sequence of the PIN

### Requirement: Locking and unlocking
The shell SHALL show the lock screen over all windows when the tablet locks and SHALL return to the previous app after a correct secret.

#### Scenario: the screen turns off and on with a lock set
- **WHEN** the screen turns off and on with a lock set
- **THEN** the lock screen is shown and apps are not reachable by touch

#### Scenario: the correct PIN is entered
- **WHEN** the correct PIN is entered
- **THEN** the previous screen is shown within 1 s

### Requirement: Rate limiting
The core SHALL refuse further attempts for 30 s after 5 wrong secrets and double the delay on each further burst.

#### Scenario: 5 wrong PINs are entered
- **WHEN** 5 wrong PINs are entered
- **THEN** the pad shows a countdown and a correct PIN during it is refused

### Requirement: Locked at boot
When a lock is configured the shell SHALL start locked.

#### Scenario: the tablet reboots
- **WHEN** the tablet reboots
- **THEN** the lock screen is shown before the Home screen

### Requirement: Rescue
The system SHALL offer an SSH-only command to remove the lock for an owner with key access.

#### Scenario: `walpad-ctl lock_clear` is run over SSH
- **WHEN** `walpad-ctl lock_clear` is run over SSH
- **THEN** the lock is removed and the next wake shows no lock
