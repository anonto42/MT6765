# Proposal: Bangladesh SIM USSD & Contacts Support

- **Requested by**: Owner request (2026-11-14)
- **Scope**:
  - Enable USSD codes (`*121#`, `*566#`, `#...#`, etc.) for all Bangladeshi SIM providers (Grameenphone, Robi, Airtel, Banglalink, Teletalk) via `AT+CUSD=1,"<code>",15`, including GSM-7 / UCS-2 decoding of `+CUSD:` responses and session cancellation (`AT+CUSD=2`).
  - Allow loading contacts stored on the SIM card (`AT+CPBS="SM"`, `AT+CPBR=?`, `AT+CPBR=<start>,<end>`) into the phone contact store via IPC (`phone_sim_contacts`) and the Phone UI.
  - Keep existing outgoing/incoming call (`ATD`, `ATA`, `AT+CHUP`/`ATH`, `AT+CLIP=1`, `AT+CLCC`) and PDU SMS (`AT+CMGF=0`, `AT+CSCA?`, `AT+CMGS`, `AT+CMGL=4`) working across all Bangladeshi number formats (`013`–`019`, `+880`, short codes).
- **Out of scope**: Kernel changes, flashing partitions, or changing `walpad-callaudio` routing.
- **Device risk**: None (no partitions written, no kernel modules loaded, no reboot required). Deployed over SSH (`scripts/deploy.sh`) with rollback support.
- **Secrets impact**: USSD codes and SIM phonebook numbers are not logged in plaintext (`AtCmd::secret` for `AT+CUSD` / `ATD`).
- **Verification on tablet**:
  - `walpad-ctl-rs` / Phone UI test against live modem (`walpad-modem start`) on `192.168.0.106` (if SIM is inserted and registered).
