# WalPad 8G recovery + server automation
#
# Primary entry points:
#   make setup      install mtkclient (clones + venv) and platform-tools
#   make watch      live USB mode watcher (BROM / preloader / WalPad-Server)
#   make probe      catch the WalPad-Server window and probe net + serial (needs root)
#   make mtk-info   try to talk to the device via mtkclient (BROM/preloader)
#   make reset      tell the device to reset (mtkclient reset)
#
# WiFi (see docs/99-status-log.md):
#   make wifi-up    bring the radio up over serial and print the DHCP address
#   make ssh        shell in over WiFi once wifi-up has run
#
# Note: sudo may be required for `probe` (network + serial) and for udev rules.

SHELL := /bin/bash
REPO  := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
VENV  := $(REPO)/tools/mtk/.venv/bin/python
MTK   := $(REPO)/tools/mtk/mtk.py
SCR   := $(REPO)/scripts

.PHONY: help setup udev watch probe console recon recon-watch host-recon super-map vendor-up wifi-up wifi-find wifi-authorize wifi-password vps-setup vps-migrate-internal vps-shell vps-status vps-stage docker-setup docker-status services-setup services-up services-down services-restart services-status services-logs dashboard terminal boot-container-kernel tailscale-up ssh net-console wifi-survey wifi-push wifi-userspace mtk-info mtk-gpt mtk-reset mtk-flash-lk clean
.DEFAULT_GOAL := help

help: ## show this help
	@grep -E '^[a-zA-Z_-]+:.*## ' $(MAKEFILE_LIST) | awk 'BEGIN{FS=":.*## "}{printf "  %-14s %s\n", $$1, $$2}'

setup: ## clone mtkclient into tools/mtk and create its venv
	@mkdir -p $(REPO)/tools
	@if [ ! -d $(REPO)/tools/mtk/mtk.py ]; then \
		git clone --depth 1 https://github.com/bkerler/mtkclient.git $(REPO)/tools/mtk; \
	else \
		echo "mtkclient already present"; \
	fi
	@python3 -m venv $(REPO)/tools/mtk/.venv
	@$(VENV) -m pip install --quiet --upgrade pip
	@$(VENV) -m pip install --quiet -r $(REPO)/tools/mtk/requirements.txt
	@echo "mtkclient ready: $(VENV) $(MTK) --help"

udev: ## install udev rules so console/mtk work without sudo (one-time)
	@sudo cp $(SCR)/99-walpad.rules /etc/udev/rules.d/99-walpad.rules
	@sudo udevadm control --reload-rules && sudo udevadm trigger
	@echo "rules installed - now unplug and replug the tablet"

watch: ## live USB mode watcher
	@bash $(SCR)/detect_mode.sh

probe: ## catch WalPad-Server window and probe (needs root: sudo make probe)
	@$(VENV) $(SCR)/probe_server.py

console: ## open interactive root shell on /dev/ttyACM0 (needs root: sudo make console)
	@$(VENV) $(SCR)/console.py

host-recon: ## collect everything the HOST can see about the tablet (no root)
	@bash $(SCR)/host_recon.sh

recon: ## full on-device recon over serial -> data/recon/ (resumable)
	@$(VENV) $(SCR)/device_recon.py

recon-watch: ## same, but keep collecting across boot windows until complete
	@$(VENV) $(SCR)/device_recon.py --watch

super-map: ## read super's layout metadata over serial (no 4 GB dump) -> data/super/
	@$(VENV) $(SCR)/vendor_up.py --no-mount

vendor-up: ## map super, then mount vendor+system read-only on the tablet
	@$(VENV) $(SCR)/vendor_up.py --watch

wifi-userspace: ## push the static wpa_supplicant/iw bundle to the tablet's SD
	@$(VENV) $(SCR)/push_wifi_userspace.py

wifi-up: ## full WiFi bring-up over serial, then report the address (run, then plug in)
	@$(VENV) $(SCR)/wifi_finish.py

wifi-find: ## find the tablet's current LAN IP by its persistent SSH host key
	@bash $(SCR)/wifi_find.sh $${SUBNET:-}

wifi-authorize: ## authorize another device: make wifi-authorize KEY_FILE=~/.ssh/id_ed25519.pub
	@test -n "$(KEY_FILE)" || { echo "KEY_FILE is required"; exit 2; }
	@bash $(SCR)/wifi_authorize.sh "$(KEY_FILE)" "$${WPD_IP:-}"

wifi-password: ## interactively set and persist the tablet's root SSH password
	@bash $(SCR)/wifi_password.sh "$${WPD_IP:-}"

vps-setup: ## provision persistent Alpine ARM64 + Tailscale (VPS_SIZE_MB=3072)
	@bash $(SCR)/vps_setup.sh

vps-migrate-internal: ## ERASE userdata and migrate Alpine to ~53 GiB internal ext4
	@IP=$${WPD_IP:-$$(bash $(SCR)/wifi_find.sh --ip-only)}; \
	 tar -C $(SCR) -cf - vps_boot.sh vps_migrate_userdata.sh | \
	 ssh -F /dev/null root@$$IP 'tar -C /mnt/sd/vps -xf -; \
	 cp /mnt/sd/vps/vps_boot.sh /mnt/sd/vps/boot.sh; chmod 755 /mnt/sd/vps/*.sh; \
	 sh /mnt/sd/vps/vps_migrate_userdata.sh'

vps-shell: ## enter the persistent Alpine environment
	@IP=$${WPD_IP:-$$(bash $(SCR)/wifi_find.sh --ip-only)}; \
	 ssh -F /dev/null -t root@$$IP 'chroot /mnt/vps /bin/bash -l'

vps-status: ## show Alpine mount, services, storage and Tailscale state
	@IP=$${WPD_IP:-$$(bash $(SCR)/wifi_find.sh --ip-only)}; \
	 ssh -F /dev/null root@$$IP 'mount | grep /mnt/vps; df -h /mnt/vps; \
	 cat /mnt/vps/etc/alpine-release; ps | grep -E "[t]ailscaled|[w]pa_supplicant"; \
	 /mnt/sd/vps/tailscale/tailscale --socket=/tmp/tailscaled.sock status 2>&1 || true'

vps-stage: ## show completed/blocked VPS stages and your next action
	@bash $(SCR)/vps_stage.sh

docker-setup: ## install restricted Docker/Compose and run a kernel compatibility test
	@bash $(SCR)/docker_setup.sh

docker-status: ## show Docker daemon and containers
	@IP=$${WPD_IP:-$$(bash $(SCR)/wifi_find.sh --ip-only)}; \
	 ssh -F /dev/null root@$$IP 'chroot /mnt/vps docker info; chroot /mnt/vps docker ps -a'

services-setup: ## install and test native Caddy + supervised services
	@bash $(SCR)/services_setup.sh

services-up services-down services-restart services-status: ## manage native VPS services
	@IP=$${WPD_IP:-$$(bash $(SCR)/wifi_find.sh --ip-only)}; \
	 ACTION='$(@:services-%=%)'; \
	 ssh -F /dev/null root@$$IP "chroot /mnt/vps /opt/walpad/services $$ACTION $${SERVICE:-}"

dashboard: ## show the persistent dashboard on the tablet display
	@IP=$${WPD_IP:-$$(bash $(SCR)/wifi_find.sh --ip-only)}; \
	 ssh -F /dev/null root@$$IP 'chroot /mnt/vps /opt/walpad/services restart dashboard'

terminal: ## switch the tablet display to the optional touch terminal
	@IP=$${WPD_IP:-$$(bash $(SCR)/wifi_find.sh --ip-only)}; \
	 ssh -F /dev/null root@$$IP 'chroot /mnt/vps /opt/walpad/services restart terminal'

boot-container-kernel: ## RAM-boot kernel with cgroup support (requires fastboot mode)
	@bash $(SCR)/boot_kernel.sh $(REPO)/build/persistence/boot-container-kernel-ramboot.img

services-logs: ## show service logs: make services-logs SERVICE=caddy
	@test -n "$(SERVICE)" || { echo "SERVICE is required (for example: SERVICE=caddy)"; exit 2; }
	@IP=$${WPD_IP:-$$(bash $(SCR)/wifi_find.sh --ip-only)}; \
	 ssh -F /dev/null root@$$IP 'chroot /mnt/vps /opt/walpad/services logs $(SERVICE)'

tailscale-up: ## authenticate/reconnect Tailscale and print its URL or IP
	@IP=$${WPD_IP:-$$(bash $(SCR)/wifi_find.sh --ip-only)}; \
	 ssh -F /dev/null -t root@$$IP '/mnt/sd/vps/tailscale/tailscale \
	 --socket=/tmp/tailscaled.sock up --hostname=walpad --netfilter-mode=off'

ssh: ## ssh in over WiFi (WPD_IP=... to override the address)
	@ssh root@$${WPD_IP:-192.168.0.100}

net-console: ## run commands over WiFi telnet instead of serial: make net-console C='uname -a'
	@$(VENV) $(SCR)/tnet.py $${WPD_IP:-192.168.0.100} $${C:-uname -a}

wifi-survey: ## probe the tablet for WiFi driver/firmware facts -> docs/wifi-survey-*.txt
	@$(VENV) $(SCR)/wifi_survey.py

wifi-push: ## copy the wifi scripts onto the tablet's SD card over serial
	@$(VENV) $(SCR)/push_file.py --mount $(SCR)/harvest_wifi_fw.sh /mnt/sd/harvest_wifi_fw.sh
	@$(VENV) $(SCR)/push_file.py $(SCR)/wifi_up.sh /mnt/sd/wifi_up.sh
	@echo "on the tablet:  sh /mnt/sd/harvest_wifi_fw.sh"

mtk-info: ## mtkclient: read SoC info / GPT (works in BROM or preloader window)
	@$(VENV) $(MTK) printgpt --socid

mtk-gpt: ## mtkclient: dump GPT table
	@$(VENV) $(MTK) gpt gpt/

mtk-reset: ## mtkclient: send reset command
	@$(VENV) $(MTK) reset

mtk-flash-lk: ## flash patched LK (lk_a) — DANGEROUS, keep lk_a.bin.orig safe
	@$(VENV) $(MTK) w lk_a firmware/MT6765/lk_a_patched.bin

clean: ## remove venv + logs
	@rm -rf $(REPO)/tools/mtk/.venv $(REPO)/tools/mtk/logs
