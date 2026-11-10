## 1. Host
- [x] 1.1 Catalogue schema validation and apps.json generation tests (fake apk)
- [x] 1.2 walpad-pkg shell tests with a stub apk
- [x] 1.3 `scripts/check.sh` passes

## 2. Device (needs the tablet; no flashing)
- [ ] 2.1 Install/uninstall round trip on the tablet
- [ ] 2.2 Interrupted install (Wi-Fi off) leaves a consistent apps.json
- [ ] 2.3 Free-space refusal with a loop-mounted small fs
