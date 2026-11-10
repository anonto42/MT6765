# App catalogue and package manager

## Why

Owner request 2026-11-10. Apps are hand-edited apps.json entries; a tablet needs install/uninstall and default apps.

## What changes

- `walpad-pkg`: wrapper over apk (install, remove, upgrade, size estimate, progress lines) with a fixed command set and timeouts.
- /etc/walpad/catalogue.json: id, name, apk packages, desktop exec, icon, approximate size, category; generates the apps.json entry on install and removes it on uninstall.
- Shell: Settings > Apps (list, size, uninstall, default apps) and an Install screen (catalogue by category, free space check).
- Default apps (browser, files, PDF, video, music, image, terminal) in /etc/walpad/defaults.json, applied to mimeapps.list and xdg-open.

## Out of scope

Flatpak/Snap (no user namespaces), sideloading untrusted packages (--allow-untrusted is refused), permissions enforcement.

## Device risk

Writes to the root filesystem on userdata (reversible with apk del); a failed install must leave apps.json valid (atomic write). Large installs need free-space checks (never fill userdata).

## Verification

On the tablet: install `mousepad` from the screen, see the icon, launch it, uninstall it, check `apk info -e mousepad` is empty and apps.json is valid JSON.
