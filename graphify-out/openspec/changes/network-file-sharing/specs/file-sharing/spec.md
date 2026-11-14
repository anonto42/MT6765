## ADDED Requirements

### Requirement: Network places
The shell SHALL let the user add, edit and remove SMB and NFS places and SHALL open them in Files.

#### Scenario: an SMB place is added with credentials
- **WHEN** an SMB place is added with credentials
- **THEN** its folders are listed in Files and files copy in both directions

### Requirement: Timeouts
Network mounts SHALL NOT block the shell or Files for more than 30 s when the server is unreachable.

#### Scenario: the server is switched off while mounted
- **WHEN** the server is switched off while mounted
- **THEN** the user sees an error within 30 s and can still use local folders

### Requirement: Credentials
Share credentials SHALL be stored in 0600 files and SHALL NOT appear in argv, logs or status.

#### Scenario: the process list is inspected during a mount
- **WHEN** the process list is inspected during a mount
- **THEN** no password is visible

### Requirement: SFTP access
When SSH is enabled the tablet SHALL accept SFTP with key authentication only.

#### Scenario: a PC runs sftp with the owner's key
- **WHEN** a PC runs sftp with the owner's key
- **THEN** directory listing and upload succeed; password login is refused

### Requirement: Storage screen
Settings SHALL show per-volume used/free space and SHALL unmount removable volumes safely on Eject.

#### Scenario: Eject is tapped on a USB stick
- **WHEN** Eject is tapped on a USB stick
- **THEN** the volume is unmounted and a notification says it is safe to remove
