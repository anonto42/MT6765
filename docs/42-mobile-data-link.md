# Mobile data link: what MediaTek's NetAgent does (static analysis, 2026-11-11)

Problem: the SIM registers, `AT+CGACT=1,1` gives an IP and `+EIF: 302, ifup, 1, <ip>, 0` arrives on
`/dev/radio/pttynwurc`, but no packet ever shows up in `rx_packets` of any `ccmniN`.

Method: static reverse engineering only (no device access). `vendor_a` was cut out of
`super-raw.img` (offset 1996488704, length 305889280) and read with `debugfs`; `aarch64`
`objdump` ran in a throwaway `debian:bookworm-slim` container. Subject:
`/vendor/lib64/libmtk-ril.so` (aarch64, BuildID `4f71cd371123b43018ccc90d9dc348d9`, not stripped
of dynamic symbols, so `NetAgentService::*` and `netagent_io_*` have names). Addresses below are
virtual addresses in that file (`.text` maps 1:1 to file offsets for `.rodata`).

## Result in one paragraph

The "NetAgent" is not a separate daemon: it is a class inside libmtk-ril (`mtkfusionrild`). It owns
**two extra AT channels**, `/dev/radio/pttynwcmd` (commands) and `/dev/radio/pttynwurc` (URCs).
When the modem announces `+EIF: <id>, ifup, <addrtype>, <ip>, ...` the RIL (1) names the interface
**`ccmni` + (`<id>` % 100)**, so id 302 is **`ccmni2`**, not `ccmni0`; (2) takes it down, brings it
up, sets the IPv4 address with `SIOCSIFADDR`; (3) **confirms the new state to the modem** with
`AT+EIF=<id>, "ifst", "up", <addrtype>` on the netagent command channel. A later `+EIF: <id>, mtu,
<n>` is applied with `ifconfig ccmni<N> mtu <n>`. The modem is the one that routes downlink packets
into the ccmni queue; the confirmation (step 3) is the only step the previous experiments did not
reproduce with the right syntax and, probably, the right channel. The kernel ccmni driver needs
nothing special: `ndo_open` does `netif_carrier_on` + `netif_tx_start_all_queues`.

## Evidence

### E1. NetAgent has its own AT channels (confidence: certain)

`netagent_io_init` @ 0x3e5250 opens two channels with `atapi_init(6, ...)`:

```
3e5314: add x3, x3, #0x9b0   ; "/dev/radio/pttynwcmd"   -> handle[0], commands
3e53a0: add x3, x3, #0xc6f   ; "/dev/radio/pttynwurc"   -> handle[1], atapi_urc_init() URCs
```

`netagent_io_send` @ 0x3e96d8 sends with `atapi_execute_command(handle[0], buf, ..., timeout 30)`;
`netagent_io_recv` @ 0x3e7820 reads `handle[1]` and only understands lines starting `+EIF: `,
`+ENAPTR: `, `+EWIFIMAC: ` (strncmp with those literals).

### E2. URC grammar (certain)

`netagent_io_recv` tokenises `+EIF: a, b, c, ...`: token 0 = decimal id (`stringToInt(tok, 10)`),
token 1 = command (`strcmp` with `ifup`, `ifdown`, `mtu`, `ifchg`, `ho`, `ipchg`, `ipadd`, `ipdel`),
for `ifup` token 2 = address type parsed as hex (`stringToInt(tok, 16)`, stored at obj+20) and
tokens 3.. = address, reason. Object command code for ifup = 0x65 (101), stored at obj+4.
Address type enum (`NetAgentService::addrTypeToString` @ 0x3c7d70): 1 = IPV4, 2 = IPV6, 3 = IPV4V6.
So `+EIF: 302, ifup, 1, 10.x.y.z, 0` means: id 302, IPv4, address, change reason 0.

### E3. Interface number = id % 100 (certain)

`NetAgentService::configureNetworkInterface` @ 0x3c2590, ifup/ifdown branch:

```
3c2630: bl netagent_io_get_if_id         ; the id of the URC (302)
3c266c: ... "[%s] push transIntfId %d to the list"   ; the full id is remembered (list at this+0x110)
3c26a8: w8 = id ; w9 = id/100 ; w8 -= w9*100       ; id % 100 stored back
3c2888: bl NetAgentService::getCcmniInterfaceName()  ; returns the global CCMNI_IFNAME_CCMNI = "ccmni"
3c2894: x3 = "%s%d"   ; snprintf(ifname, 16, "%s%d", "ccmni", id % 100)
```

`configureMTUSize` @ 0x3c3b10 builds the name the same way (`"%s%d"`) and runs
`asprintf("ifconfig %s mtu %s")` + `system()`.

### E4. Interface bring-up order (high)

Same function, state 1 (up): log `"[%s] set network interface down before up"` ->
`setNwIntfDown` -> `ifc_reset_connections` -> `getIpv4Address` (from the URC object) ->
`nwIntfSetFlags(fd, ifr, set=IFF_UP)` (`SIOCGIFFLAGS` 0x8913 then `SIOCSIFFLAGS` 0x8914 in
@0x3c76c8) -> `nwIntfSetAddr` (@0x3c7810: `inet_addr`, **only `SIOCSIFADDR` 0x8916**, no netmask,
no route, no gateway; routing is Android netd's job, i.e. ours). For IPv6 it also runs the RS
timer. Down: `setNwIntfDown`, `resetNwIntfMtu`.

### E5. The confirmation (certain for the text, high for the necessity)

After the ioctls, `configureNetworkInterface` @ 0x3c2cb8 calls
`NetAgentService::confirmInterfaceState(ifid, up?1:0, addrtype)` @ 0x3c8ea8, which maps the short id
back to the full id (`getTransIntfId(2)` finds the list entry 302 because 302 % 100 == 2), calls
`netagent_io_cmd_ifst_alloc(tid, state, addrtype)` and `netagent_io_send`. In
`netagent_io_send` command code 1 formats (@0x3e9900..0x3e9950):

```
"AT+EIF=%d, "ifst", %s, %d"      %d = full id (302), %s = "up" or "down" WITH the double quotes
                                 (literals at 0x109ecf and 0x107a5c are  "up"  and  "down" ), %d = addr type
```

i.e. the exact line is `AT+EIF=302, "ifst", "up", 1` (spaces after the commas), sent on
`/dev/radio/pttynwcmd` with a 30 s timeout. It gets nothing else around it. The earlier attempts
(`CME ERROR 50`, `65535`) used a different spelling (no quotes around `up`) and/or the main AT
channel; neither matches this code. The modem-side meaning (the AP side of the ifst is up, start
routing) is inferred from the names and from the fact that the RIL never skips the step; the
modem firmware is not available for inspection.

### E6. Capability sync at NetAgent start (high)

`NetAgentService::syncCapabilityToModem` @ 0x3c1648 queues a request 0xc9; `netagent_io_send`
handles 0xc9 @ 0x3ea5ec by formatting `"AT+EIFCONFIG=%d"` with the constant **3**. Sent once when
NetAgent starts, on the same command channel. Meaning (inferred): the AP handles interface
configuration (ifst/ip add/del). Without it the modem may use a different mode.

### E7. What the RIL itself does to activate (high for syntax, low for details)

`RmcDcCommonReqHandler::requestSetupDataCall` @ 0x233a20 -> `activatePdn` @ 0x2304c0 sends (format
at 0x2305c4, called at 0x23409c with the APN, the apn-type string from `getProfileType`
("default", "mms", "supl", ...), and the row index from `getCmdIndexFromApnTable`):

```
AT+EAPNACT=1,"<apn>","<apn type, e.g. default>",<apn table index>      ; answer +CGEV: ME PDN ACT <aid>[,<reason>[,<otherAid>]]
```

and before that `requestSyncApnTable` @ 0x241830 uploads the table (`AT+EAPNLOCK=1`, bare
`AT+EAPNSET` to clear, per APN `AT+EAPNSET="<apn>",1,"<user>","<pass>"` and
`AT+EAPNSET="<apn>",2,"type=default,hipri,mms,...;protocol=IP;roaming_protocol=IP;authtype=0;carrier_enabled=1;max_conns=0;max_conns_time=0;wait_time=0;bearer_bitmask=0;inactive_timer=0"`
with an `+EAPNSET:` answer, then `AT+EAPNLOCK=0`). That explains `CME 5873` for bare
`AT+EAPNACT` experiments: the modem's APN table was empty. After activation `updatePdnInformation`
@ 0x2320c8 sends `AT+EPDN=<aid>,"ifst",<protocol bitmap + 16>` and parses
`+EPDN: <aid>,"new"|"update"|"err",<rat>,<transIntfId>,<mtu>,<addrtype>,<addr1>[,<addr2>]`
(so the modem hands out the transIntfId here too). The classic `AT+CGDCONT`/`AT+CGACT` route
that `walpad-modem` uses is not what the RIL sends, but it clearly makes the modem create the PDN
and the `+EIF ifup` URC. The table layout of `AT+EAPNSET` is the least certain part (the meaning
of its second integer is deduced from the `"ims",1/2` literals, not proven).

### E8. Kernel side (certain)

`tools/kernel-mt6765-4.19/drivers/misc/mediatek/ccmni/ccmni.c`: `ccmni_open` (line 400) only needs a
valid md/index, then `netif_carrier_on` and `netif_tx_start_all_queues`. `ccmni_ioctl`
(line 694) handles `SIOCSTXQSTATE` (rild/thermal throttle), `SIOCCCMNICFG` (iRAT md switch, used by
`libifcutils_mtk` `ifc_ccmni_md_cfg`, **not called by the RIL**: `libmtk-ril` imports only
`ifc_close/disable/get_ifindex/init/reset_connections/add_address/del_address/remove_default_route`),
`SIOCFWDFILTER` and `SIOCACKPRIO`. Nothing in the kernel gates rx on userland. `ccci_mdinit`'s
"ccmni0..3" strings are libccci_util's table of device names, not a ccmni setup. `netdagent` only
does tc/iptables for `ccmni0..2` (throttling, firewall) and no bring-up.

## The recovered sequence (what `walpad-modem data on` now does)

All on the AT channels of the gsm0710muxd (`/dev/radio/*`):

| # | Channel | Line | Source |
|---|---------|------|--------|
| 0 | background | read `pttynwurc` into a file BEFORE step 4 (the ifup URC arrives during the CGACT) | E1 |
| 1 | pttycmd1 | `AT+EDALLOW=1` | previous work |
| 2 | pttynwcmd | `AT+EIFCONFIG=3` (once per boot) | E6 |
| 3 | pttycmd1 | `AT+CGDCONT=1,"IP","<apn>"` | previous work |
| 4 | pttycmd1 | `AT+CGACT=1,1` | previous work |
| 5 | pttycmd1 | `AT+CGPADDR=1`, `AT+CGCONTRDP=1` (ip, dns) | previous work |
| 6 | pttynwurc | wait for `+EIF: <id>, ifup, <type>, <ip>, <reason>` (8 s), then `+EIF: <id>, mtu, <n>` | E2 |
| 7 | host | hook: `ip link set ccmniN down; addr flush; link set up; addr add <ip>/32; mtu; route default dev ccmniN metric 500`, N = id % 100 | E3 E4 |
| 8 | pttynwcmd | `AT+EIF=<id>, "ifst", "up", <type>` | E5 |

`walpad-modem data off`: `AT+EIF=<id>, "ifst", "down", <type>`, hook `down`, `AT+CGACT=0,1`.

No ifup URC within 8 s: the id 302 seen on this device is assumed (logged as unverified).
Without `pttynwcmd`, the `AT+EIF` lines go to `pttycmd1`. The confirmation failing does not
undo the link (state stays on; `[modem]` line says so), so the owner can still probe by hand.

Confidence: bring-up and naming (E3, E4) high; exact confirmation text (E5) high; that the
confirmation (plus the channel) is what unblocks downlink: medium (about 60 %). Not proven because
the modem firmware cannot be read here. The `AT+EAPNACT` route (E7) is documented but not
implemented (low confidence on the `AT+EAPNSET` table syntax).

## Still unknown

1. Whether the modem accepts `AT+EIF=...ifst` before/after the MTU URC, and what its own state
   machine does on `ifst up` (firmware not analysed).
2. Whether the CGACT-created PDN needs the AP to also issue `AT+EPDN=1,"ifst",<bitmap+16>` (E7)
   like the RIL does after `EAPNACT`.
3. Number of `ccmni` devices on this kernel build (`ls /sys/class/net`); ids above 20 map to
   `ccmni(id%100)`, ids 3xx probably mean "MD1 bearer class 3".
4. IPv6: `+EIF ... ipadd` URCs and the RS timer are not implemented (IPv4 only).

## Device experiment list (SSH, safe order, nothing flashes or erases)

Pre: `cat /sys/kernel/ccci/lk_md` must say `Enabled` (reboot normally otherwise); `walpad-modem
status`; the SIM registered (`AT+CEREG?` on `/dev/radio/pttycmd1`). `ls /dev/radio` must list
`pttynwcmd` and `pttynwurc`. All experiments with `busybox stty -F <dev> raw -echo` and
`busybox cat`/`printf '..\r'`; one opener per channel at a time, so stop `walpad-core`'s modem
poll first (`walpad-ctl` pause, or stop the core) before using the AT channels by hand.

1. Does the deployed script work? `walpad-modem data on; echo rc=$?; ip -s link show; cat /run/walpad/nwurc.log`
   then `ping -c3 -I ccmni2 1.1.1.1` and `ip -s link show ccmni2` (rx_packets must grow).
   Check the log: `[modem] data on: transIntfId 302 -> ccmni2`, `interface confirmed`.
2. If rx stays 0 and the log says "did not accept": run by hand on the netagent channel
   (`/dev/radio/pttynwcmd`): `AT+EIF=302, "ifst", "up", 1`; read the reply. Then ranked variants
   (try each, `ping` after each):
   a. same line on `/dev/radio/pttycmd1` (what the script does as fallback);
   b. `AT+EIF=302,"ifst","up",1` and `AT+EIF=302, "ifst", up, 1` (spacing/quote variants);
   c. `AT+EIF=2, "ifst", "up", 1` (short id);
   d. `AT+EPDN=1,"ifst",17` then `AT+EPDN=1,"ifst",19` (E7: the RIL's post-activation query; the
      reply `+EPDN: 1,"new",<rat>,<tid>,<mtu>,<type>,<ip>` also proves the tid).
3. If the confirmation is accepted but rx stays 0: check the interface really is the right one,
   compare `cat /proc/net/dev` of all ccmni, `dmesg | grep -i -E 'ccmni|ccci'`
   (`ccmni2_Open:cnt=...` line shows the driver opened it), `ip addr show ccmni2` (must show the
   /32), `ip route`, then send a packet with `ping -I ccmni2 <dns ip>` and watch `tx_packets`,
   `tx_dropped`, `tx_errors` of ccmni2 (tx counters stuck = queue stopped: `dmesg` for
   `queue_state`/`TX_IRQ`; the kernel stops the queue when the modem reports `HIF_STATE` off).
4. Fall back to the RIL path: `AT+EAPNLOCK=1`, `AT+EAPNSET`, `AT+EAPNSET="gpinternet",2,"type=default;protocol=IP;roaming_protocol=IP;authtype=0;carrier_enabled=1;max_conns=0;max_conns_time=0;wait_time=0;bearer_bitmask=0;inactive_timer=0"`,
   `AT+EAPNLOCK=0`, then `AT+EAPNACT=1,"gpinternet","default",<n>` (n = 0, then 1, then the number the
   `+EAPNSET:` reply printed). Expect `+CGEV: ME PDN ACT <aid>` and an `+EIF ... ifup` with another id;
   then repeat the confirmation with that id. Needs `AT+EGDCONT`-free state: `AT+CGACT=0,1` first.
5. Run the real RIL as an oracle (read-only for the modem NV, same method as `walpad-gps`): start
   `mtkfusionrild` through the linker with the property shim, and watch
   `/var/log/walpad-android.log` for `NetAgentService`/`NetAgent_IO` lines
   (`AT> AT+EIF=302, "ifst", "up", 1`, `AT< OK`). Most expensive, only if 2-4 fail, because the
   RIL then also wants a Java/binder side to trigger a data call.
6. Dump what the modem thinks: `AT+CGDCONT?`, `AT+CGACT?`, `AT+EPDN=1,"addr"`,
   `AT+EPDN=1,"ran"`, `AT+CGCONTRDP=1`, `AT+CGEREP?` (add `AT+CGEREP=2,1` to see `+CGEV` events).
