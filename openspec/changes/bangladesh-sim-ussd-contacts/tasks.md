# Tasks: Bangladesh SIM USSD & Contacts Support

## Host checks
- [x] Add `PhoneUssd` / `PhoneUssdCancel` / `PhoneSimContacts` IPC commands and `UssdStatus` / `PhoneStatus.ussd` to `walpad-proto`
- [x] Add `+CUSD:` and `+CPBR:` parsers (including UCS-2 hex decoding) to `crates/walpad-core/src/phone/parse.rs`
- [x] Implement `Op::Ussd`, `Op::UssdCancel`, and `Op::SimContactsRange`/`Op::SimContactsRead` in `crates/walpad-core/src/phone/mod.rs` and wire them in `crates/walpad-core/src/daemon.rs`
- [x] Wire USSD response display and "SIM" contacts button in `crates/walpad-shell` (`app.rs`, `phone_ui.rs`, `app/phone_flow.rs`)
- [x] Run `bash scripts/check.sh` and confirm `ALL CHECKS PASSED`

## Device checks
- [x] Deploy `walpad-core`, `walpad-shell` to tablet (`192.168.0.106`) via `scripts/deploy.sh`
- [x] Verify `walpad-core` and `walpad-shell` are live and responsive
