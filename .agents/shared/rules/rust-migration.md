# Rust Code Rules

1. Contracts: `docs/rust/CONTRACTS.md` changes only when the task needs it; mark additions with
   the owner request and date.
2. Every hardware read returns Option/Result; missing sensors are "unavailable". Battery estimate
   only tagged `source: "voltage_estimate"`.
3. Every blocking operation (spawn, socket, sysfs write, X request, AT command) has a timeout.
4. No arbitrary command execution: apps launch by id from `/etc/walpad/apps.json`; helper scripts
   by fixed path.
5. No secret in logs/argv/fixtures/commits; passphrases go through stdin or control sockets;
   use `redact()`.
6. New dependencies: pure Rust (no -sys crates), one-line justification in the commit.
7. Goldens: `UPDATE_SNAPSHOTS=1 scripts/rust.sh test -p walpad-shell`, then look at the PNGs.
8. `scripts/check.sh` (fmt, clippy -D warnings, tests incl. Xvfb, aarch64 build, qemu smoke, shell
   tests, fbmirror logic, secrets grep) must pass. The X tests can time out under load; rerun
   the single test before suspecting the code.
9. Cargo runs through `scripts/rust.sh` (CI docker image) when no host cargo exists.
10. Code structure: keep one responsibility per file, `<= 300` lines per production `.rs` file
    (excluding pure wire-protocol enums such as `walpad-proto/src/command.rs`), and keep main
    code and unit tests in separate files (`tests.rs` / `tests_*.rs` / `tests/`).
