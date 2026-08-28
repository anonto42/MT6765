# Walton WalPad 8G — MT6765 Recovery & Low-Level Learning Project

A real-world repair of a bricked **Walton WalPad 8G** tablet
(**MediaTek MT6765 / Helio A22**, eMMC, Android 11, A/B partitions) — and, along
the way, a hands-on course in how mobile firmware, bootloaders, and the boot
chain actually work.

This repo holds the stock firmware images, the recovery tooling that was
written to drive the repair, and a documentation set that goes deep on every
concept involved — from "what is a preloader" to "how to patch a Thumb-2
instruction in the bootloader by hand."

> **Status (2026-08-20): the built-in WiFi works.** The tablet associates with a
> WPA2 network, takes a DHCP lease, reaches the internet, and serves SSH — so a
> shell no longer needs the USB cable:
>
> ```
> $ ssh root@192.168.0.100 'uname -a'
> Linux (none) 4.19.127 #1 SMP PREEMPT Fri May 6 19:07:29 CST 2022 aarch64 GNU/Linux
> ```
>
> **Boot persistence is working.** The boot ramdisk now launches the SD-card
> WiFi stack automatically; after a cold reboot the tablet associates, takes a
> DHCP lease, and starts SSH without USB commands. The DHCP address may still
> move until the router has a reservation for the tablet.
>
> The tablet boots a custom **"WalPad-Server" Linux** firmware that exposes a
> root shell on `/dev/ttyACM0` plus USB ethernet (RNDIS + ECM). With the battery
> low it power-cycles through BROM → preloader → WalPad-Server; BROM mode (hold
> Vol Up + Power + plug USB) is reachable for flashing. See
> [docs/99-status-log.md](docs/99-status-log.md) for the full field log.

---

## WiFi: how to get a shell

**Every boot, in order.** Nothing here survives a reboot.

```
make wifi-up      # start this FIRST, then plug the USB cable in
make ssh          # once wifi-up prints an address
```

`make wifi-up` waits for the USB window, brings the whole radio stack up, runs
`wpa_supplicant`, takes a DHCP lease, starts dropbear, and prints the address.
The slow part — a 4.3 MB supplicant bundle — is cached on the SD card, so only
the first run pays for it.

After that the cable is optional. Two ways in over the network:

| | |
|---|---|
| `make ssh` | key-based SSH (`WPD_IP=… make ssh` to override the address) |
| `make net-console C='uname -a'` | one-shot command over the firmware's telnetd |

**Prefer the network over the serial console once `wlan0` is up.** The CDC-ACM
console wedges regularly — after a failed radio power-on it stops draining its
endpoint, writes time out, and only a physical power-cycle recovers it. The
network path does none of that and is far faster.

### Setup, once

```
printf 'SSID="YourNetwork"\nPSK="yourpassword"\n' > data/wifi/wifi.conf
```

That file is gitignored; `data/wifi/wifi.conf.example` shows the format. It is
pushed to the tablet's SD card and never leaves the two machines.

### How it actually works

The radio is a MediaTek **CONNSYS/CONNAC** block, and it does not come up by
insmodding a driver. `scripts/mtk_stack_up.sh` walks the sequence Android's
init would, and **the order is load-bearing**:

1. **Loop-mount `/vendor` and `/system`** off cached images on the SD card —
   the firmware, modules and Android userspace all live there.
2. **`insmod wmt_drv`** — and nothing else yet.
3. **Fake the Android property service.** `wmt_loader` and `wmt_launcher` reach
   it through libcutils' unversioned `property_get`/`property_set`, so
   `tools/propshim/` builds a freestanding aarch64 `LD_PRELOAD` shim that backs
   those with a flat file. They run under `/system/system/bin/bootstrap/linker64`,
   because the normal `linker64` is an APEX symlink that does not resolve here.
4. **Run `wmt_loader`**, which detects the chip and publishes
   `vendor.connsys.driver.ready`.
5. **Only now insmod `wmt_chrdev_wifi` and `wlan_drv_gen4m`.** Loading them any
   earlier leaves the CONNSYS EMI base unset and the firmware download dies with
   `gConEmiPhyBaseFinal invalid`.
6. **Start `wmt_launcher` and leave it running.** The kernel asks *userspace*
   for the ROM patch, and a request nobody answers wedges the driver's single
   command buffer for the rest of the boot — unrecoverable without a power-cycle.
7. **`mknod /dev/wmtWifi`** from the major:minor in sysfs. There is no `ueventd`
   here, so nothing creates the node; a plain `echo 1 > /dev/wmtWifi` creates a
   *regular file* that swallows every write silently.
8. **`echo 1 > /dev/wmtWifi`** → firmware downloads, `wlan0` appears.

Then `scripts/wifi_assoc.sh` associates and gets a lease. It uses the musl
`wpa_supplicant` from `data/wifi-userspace/`, not the vendor one: Android's
`/vendor/bin/hw/wpa_supplicant` needs `libnl.so`, which ships in the
product/system_ext images this firmware does not carry. The Alpine bundle brings
its own `libnl-3` and sidesteps that entirely.

The authoritative source for every argument and ordering rule above was the
vendor's own init files — `/vendor/etc/init/init_connectivity.rc` and
`init.wlan_drv.rc`. Three separate bugs came from guessing instead of reading
them.

---

## What's next

| | Why it matters |
|---|---|
| **Persistence** | **Fixed 2026-08-27.** `/init` starts an embedded `wifi_boot.sh`, which mounts the SD card, runs the proven stack, and restores persistent SSH keys. A real reboot returned WiFi and SSH without host intervention. |
| **Close telnetd** | The firmware runs a passwordless root shell on port 23. Over the USB tether that was a private link; on WiFi it is offered to the whole LAN. SSH keys are installed, so nothing depends on it anymore. |
| **Stable identity** | The Dropbear host key now persists on the SD card. DHCP can still move the address; add a router reservation for the tablet's WiFi MAC. |
| **NVRAM** | `glLoadNvram: Nvram not available` — the MAC/calibration partition is not mounted, so the MAC is a driver fallback and TX calibration is whatever the firmware defaults to. It associates fine; RF performance is unverified. |


## Repository layout

```
walton/
├── README.md                  ← you are here
├── docs/
│   ├── 99-status-log.md               running status / field notes (start here)
│   └── 10-wifi-plan.md                plan for onboard WiFi (no USB dongle)
├── scripts/                   ← tooling for the recovery + server
│   ├── console.py                    root shell over /dev/ttyACM0 (interactive or --exec)
│   ├── serial_probe.py               shared console engine (framing, resumability)
│   ├── device_recon.py               49-probe full device inventory -> data/recon/
│   ├── host_recon.sh                 host-side inventory, no root -> data/host/
│   ├── wifi_survey.py                the WiFi slice of the recon catalogue
│   ├── push_file.py                  copy a host file to the tablet over serial
│   ├── wifi_finish.py                one-shot: stack up + associate + sshd  ← start here
│   ├── mtk_stack_up.sh               (on-tablet) the CONNSYS bring-up sequence
│   ├── wifi_assoc.sh                 (on-tablet) wpa_supplicant + DHCP + dropbear
│   ├── push_wifi_userspace.py        send the musl supplicant bundle to the SD card
│   ├── tnet.py                       drive the tablet over WiFi telnet, not serial
│   ├── reboot_now.py                 reboot from the console while it still answers
│   ├── vendor_up.py                  loop-mount /vendor and /system off the SD card
│   ├── harvest_wifi_fw.sh            (on-tablet) pull wifi fw/modules off /vendor
│   ├── wifi_up.sh                    (on-tablet) superseded by wifi_assoc.sh
│   ├── probe_server.py               catch the WalPad-Server window, probe net + serial
│   ├── detect_mode.sh                live USB mode watcher (BROM / preloader / server)
│   ├── instant_catch.py              race the boot window, run mtkclient the moment it opens
│   ├── flash_b_slot.py               clone slot A images onto slot B
│   ├── boot_kernel.sh                RAM-boot the stock kernel via fastboot
│   ├── charge_keeper.sh              watch/hold the device in a charging state
│   └── usb_utils.sh                  shared helpers for the above
├── data/                     ← collected device data (recon dumps)
│   ├── host/                         what the host can see (USB descriptors, links)
│   ├── recon/                        on-device probes + INDEX.md + KEY-FACTS.md
│   ├── super/                        super-partition layout metadata
│   ├── wifi/                         property shim + wifi.conf (gitignored)
│   └── wifi-userspace/               musl wpa_supplicant bundle for the tablet
├── tools/propshim/           ← freestanding aarch64 LD_PRELOAD property shim
├── tools/mtk/                ← mtkclient (cloned; venv at tools/mtk/.venv)
│   └── mtkclient/config/usb_ids.py   patched: added PID 0x2008 (WalPad-Server)
├── Makefile                 ← automation: make watch / probe / mtk-info / setup
├── flake.nix                ← optional Nix devshell (reproducible toolchain)
├── firmware/MT6765/         ← (planned) stock firmware images
├── tools/platform-tools/    ← (planned) Android adb + fastboot
└── archive/                 ← (planned) original vendor stock-firmware zip
```

## Quick start

New to the project? Read [docs/99-status-log.md](docs/99-status-log.md) first.

Just need to *do* something:

| I want to… | Run |
|---|---|
| **Get on WiFi (run first, then plug in)** | `make wifi-up` |
| **Shell in over WiFi** | `make ssh` |
| **Find its current DHCP address** | `make wifi-find` |
| **Authorize another device's SSH key** | `make wifi-authorize KEY_FILE=/path/to/id_ed25519.pub` |
| **Set a persistent SSH password** | `make wifi-password` |
| **Provision Alpine + Tailscale** | `make vps-setup` |
| **Enter persistent Alpine** | `make vps-shell` |
| **Show VPS progress and my next task** | `make vps-stage` |
| **Move Alpine to internal userdata** | `make vps-migrate-internal` (**erases Android userdata**) |
| **Install/test Docker compatibility** | `make docker-setup` |
| **Install/test native service hosting** | `make services-setup` |
| **Manage hosted services** | `make services-up`, `make services-status`, `make services-down` |
| **Read a service log** | `make services-logs SERVICE=caddy` |
| **One command over WiFi** | `make net-console C='uname -a'` |
| Watch USB mode live | `make watch` (or `bash scripts/detect_mode.sh`) |
| Get a root shell on the tablet | `sudo python3 scripts/console.py` (interactive) |
| Run one command on the tablet | `sudo python3 scripts/console.py --exec "uname -a"` |
| Probe the server window | `sudo make probe` |
| Flash patched LK (dangerous) | `sudo python3 scripts/instant_catch.py w lk_a firmware/MT6765/lk_a_patched.bin` |
| Mirror images to slot B | `sudo python3 scripts/flash_b_slot.py` |
| RAM-boot the stock kernel | `scripts/boot_kernel.sh` (device in fastboot) |
| Watch a charging run | `bash scripts/charge_keeper.sh 60` |
| Drop the sudo prompts (one-time) | `make udev`, then replug |
| Inventory the device from the host | `make host-recon` |
| Full on-device inventory | `make recon-watch` |
| Survey the tablet's WiFi support | `make wifi-survey` |
| Push the WiFi scripts to the SD card | `make wifi-push` |
| Push the supplicant bundle to the SD card | `make wifi-userspace` |

All scripts locate the repo from their own path, so they work no matter where
the repo lives. Network/serial tools need root (`sudo`).

The stock kernel cannot run OCI containers safely because it lacks the devices
cgroup controller. The supported hosting path is therefore the native service
manager plus Caddy. Service definitions live in `config/services/*.conf`; Caddy
routing lives in `config/services/Caddyfile`.

## Automation

`make help` lists targets. A Nix devshell (`flake.nix`) is provided for a
reproducible toolchain; a `moon.yml` task file can be added if you prefer
moon over make.

## ⚠️ Safety

- **Never** flash a `preloader` or `lk` you are unsure of — a bad one means
  BROM-only recovery (or a paperweight). Keep `lk_a.bin.orig` as the escape hatch.
- **Never** do blind PMIC charger pokes on a lithium cell. See
  [docs/06-charging-lockout.md](docs/06-charging-lockout.md).
- Keep the full stock images intact; they are the only known-good restore point.
