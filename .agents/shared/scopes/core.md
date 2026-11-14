# Scope: walpad-core (crates/walpad-core)

Hexagonal / Feature-Sliced layout under `src/`: `domain/` (`brain`, pure `logic/*`),
`features/` (`connectivity/`, `device/`, `location/`, `telephony/`), `adapters/` (`shadow`,
`xbridge`), `app/` (`daemon` composition root), and `shared/` (`fake`). Crate root (`lib.rs`)
re-exports module names for public/test compatibility.

Single-thread poll loop daemon: inputs (evdev by name, late devices re-discovered), Brain (pure
logic -> Effects), xbridge (XTEST pointer/keys, struts, focus), wpa (Wi-Fi control socket),
hotspot (walpad-hotspot script), apps (spawn by id, reap, crash detection), IPC server
`/run/walpad/control.sock` (NDJSON, `docs/rust/CONTRACTS.md`). Pokes fbmirror via
`/run/walpad/fbwake` on input. Helper processes are reaped; zombies never count as running.
