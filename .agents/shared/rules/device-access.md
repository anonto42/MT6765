# Device Access (SSH over Wi-Fi)

- `ssh -F build/ssh/config walpad '<cmd>'` (update HostName when DHCP changes; find the IP with
  `bash scripts/wifi_find.sh --ip-only`). Host keys live in `build/ssh/known_hosts` (each flash
  makes a new key); never edit `~/.ssh/known_hosts`. Root PATH over SSH lacks /usr/local/bin:
  use full paths or `export PATH=$PATH:/usr/local/bin`.
- Deploy Rust binaries: `scripts/build_rust.sh` then
  `WALPAD_SSH_EXTRA="-o UserKnownHostsFile=build/ssh/known_hosts" scripts/deploy.sh --host <ip> [files]`
  (checks hashes, keeps other binaries, prints "verified live"). Restart: kill walpad-core (its
  supervisor restarts it), `walpad-ctl-rs restart-shell`.
- Scripts/config: `ssh walpad 'cat > /path.new && mv /path.new /path' < file`, `sh -n` first.
- Screenshot: `walpad-ctl-rs screenshot` -> `/root/screenshots/*.xwd`; convert on the host with
  `convert xwd:file -crop 800x1280+0+0 out.png` (landscape: 1280x800).
- Logs: `/var/log/walpad-*.log`, `/var/log/walpad-android.log` (vendor daemons via the logdw
  socat stand-in), `dmesg`.
- Every remote command gets a `timeout`; never leave tests running that could hang the device.
- Everything deployed must also be committed in the source (config/rootfs/..., build_rootfs.sh
  packages) so the next image build contains it.
