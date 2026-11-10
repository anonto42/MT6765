# VPN, proxy and DNS

## Why

Owner request 2026-11-10. Users expect VPN, proxy and private DNS; none exists, and the kernel has no WireGuard module.

## What changes

- `walpad-vpn` script: up/down/status/import for WireGuard (wireguard-go over TUN, wg, wg-quick style) and OpenVPN; configs in /var/lib/walpad/vpn (0700).
- Kill switch with legacy iptables (drop traffic outside the tunnel), DNS override of resolv.conf while connected.
- Shell: Settings > Network > VPN screen (import from Files, toggle, status), DNS server field, HTTP proxy field (exported to launched apps).
- Core command `vpn {on,off}`, status field `status.vpn`.

## Out of scope

Per-app VPN, IPsec/IKEv2 (strongswan later), Tor, VPN inside the Kali chroot beyond sharing the host routes.

## Device risk

Changes routes and iptables: always revert on stop or failure with a timeout; a bad kill switch can cut off SSH: keep the LAN SSH exception and a 60 s safe-revert option. Private keys never in logs or argv.

## Verification

On the tablet: import a WireGuard test config, toggle on, `wg show` has a handshake, `curl` shows the endpoint IP; toggle off and check the default route and iptables are restored.

## Open questions

- Do the TUN device node (/dev/net/tun) and iptables owner match work in the chroot as expected?
