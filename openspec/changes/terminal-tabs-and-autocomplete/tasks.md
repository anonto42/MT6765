## 1. Research (done, evidence in docs/46-terminal-tabs-and-autocomplete.md)
- [x] 1.1 Confirm terminal tabs already exist (`wmbar.rs`, `multitasking-windows`, apps.json
      `max_instances`) by reading the code, not assuming
- [x] 1.2 Real `apk` check (docker, `--arch aarch64`, Alpine 3.24 main/community/edge-testing) for
      `tabbed`: not packaged anywhere
- [x] 1.3 Real check that Alpine's `st` package (vanilla upstream 0.9.3, no patches) supports
      `-w windowid` (XEmbed) regardless
- [x] 1.4 Real `apk` check for `fish`, `zsh`, `zsh-autosuggestions`, `zsh-syntax-highlighting`,
      `zsh-completions`, `tmux`: all available aarch64; picked `fish` (no plugin wiring needed)

## 2. Host (config/shell-script/docs only, no Rust touched)
- [x] 2.1 `scripts/build_rootfs.sh`: add `fish` to `GUI_PKGS`, kept out of the `NO_PY` trim list
- [x] 2.2 `config/rootfs/etc/walpad/apps.json` and `config/rootfs-nopython/files/etc/walpad/apps.json`:
      `terminal` entry's `exec` gains `-e fish`; `kali` entry unchanged
- [x] 2.3 `docs/46-terminal-tabs-and-autocomplete.md`: findings and the tabbed/wmbar decision
- [ ] 2.4 `tests/shell/run.sh` passes (package-list and apps.json assertions unaffected)

## 3. Device (needs the tablet; no flashing by this change)
- [ ] 3.1 Rebuild + flash the image (owner-approved), then confirm the window-bar tab strip for a
      multi-session terminal (already-shipped `multitasking-windows` behaviour)
- [ ] 3.2 Confirm `fish` autosuggestions/tab completion work with the touch keyboard and selection
      toolbar in `st`
