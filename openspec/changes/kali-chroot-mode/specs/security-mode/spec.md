## ADDED Requirements

### Requirement: Verified install
The system SHALL refuse to unpack a Kali rootfs whose SHA-256 does not match the published checksum.

#### Scenario: a tampered tarball is given
- **WHEN** a tampered tarball is given
- **THEN** install stops with a mismatch message and nothing is unpacked

#### Scenario: the checksum list cannot be fetched
- **WHEN** the checksum list cannot be fetched
- **THEN** install refuses unless the user supplies a checksum

### Requirement: Mount handling
The system SHALL bind-mount /dev, /dev/pts, /proc, /sys, /tmp and storage under /mnt for the chroot and SHALL unmount them in reverse order on exit or on request.

#### Scenario: walpad-kali umount runs
- **WHEN** walpad-kali umount runs
- **THEN** none of the chroot paths remain in /proc/mounts and the host /dev is intact

### Requirement: Terminal and X11 tools
The system SHALL provide a terminal profile that opens a shell in the chroot and SHALL let GUI tools in the chroot display in the existing X session with the system keyboard.

#### Scenario: a GUI tool is started in kali-terminal
- **WHEN** a GUI tool is started in kali-terminal
- **THEN** its window appears in the session and text input works

### Requirement: Tool sets
The system SHALL install Kali tool sets only on request through apt metapackages (kali-linux-core, kali-tools-top10 and the per-menu kali-tools-* groups) and SHALL show the free space before installing.

#### Scenario: kali-tools-top10 is requested with 2 GiB free
- **WHEN** kali-tools-top10 is requested with 2 GiB free
- **THEN** the install is refused with a space message

### Requirement: Authorised-use notice
The first launch SHALL show a notice that the mode is for the owner's own equipment and authorised targets, and the mode SHALL NOT start at boot.

#### Scenario: the mode is launched for the first time
- **WHEN** the mode is launched for the first time
- **THEN** the notice must be accepted before the shell opens

#### Scenario: the tablet reboots
- **WHEN** the tablet reboots
- **THEN** no Kali process or mount exists

### Requirement: Honest hardware limits
The Security testing page SHALL state that monitor mode, packet injection and HID emulation are unavailable until suitable modules exist.

#### Scenario: the page is opened
- **WHEN** the page is opened
- **THEN** each unavailable capability is listed with its reason
