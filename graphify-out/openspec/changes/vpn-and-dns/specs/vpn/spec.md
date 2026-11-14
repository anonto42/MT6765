## ADDED Requirements

### Requirement: WireGuard
The system SHALL bring up a WireGuard tunnel with wireguard-go on a TUN device from an imported config and SHALL report handshake time and bytes.

#### Scenario: a valid config is enabled
- **WHEN** a valid config is enabled
- **THEN** the interface is up, the default route uses it and status shows a handshake

### Requirement: Clean teardown
The system SHALL restore routes, DNS and iptables to the state before the tunnel when it is disabled or fails.

#### Scenario: the tunnel is turned off
- **WHEN** the tunnel is turned off
- **THEN** `ip route` and `iptables -S` equal the saved pre-tunnel state

### Requirement: Kill switch
When the kill switch is on, the system SHALL block non-tunnel traffic except loopback, DHCP and the VPN endpoint.

#### Scenario: the tunnel process is killed
- **WHEN** the tunnel process is killed
- **THEN** outbound traffic to the internet fails until the tunnel returns or the switch is disabled

### Requirement: Secret handling
Private keys and passwords SHALL be stored with mode 0600 and SHALL NOT appear in logs, argv or status.

#### Scenario: `ps` and the logs are searched for the key
- **WHEN** `ps` and the logs are searched for the key
- **THEN** no match is found

### Requirement: DNS and proxy
The user SHALL be able to set DNS servers and an HTTP proxy that apply to the system and to launched apps.

#### Scenario: a DNS server is set
- **WHEN** a DNS server is set
- **THEN** resolv.conf uses it and name lookups succeed
