# Claude Code Context (WalPad 8G)

Source of truth: `.agents/shared/`. Files in `.claude/` are one-line `@` proxies; never put
content there.

## Always-Loaded

@.claude/agent-startup.md
@.claude/memory/MEMORY.md
@.claude/rules/README.md

## Conditional: Rules (load only what matches)

- Flashing / partitions / backups / mtkclient: `.agents/shared/rules/device-safety.md`
- SSH, deploy, screenshots, logs on the tablet: `.agents/shared/rules/device-access.md`
- Rust crates: `.agents/shared/rules/rust-migration.md`
- Android vendor daemons (Wi-Fi, modem, GPS, BT, camera): `.agents/shared/rules/vendor-daemons.md`
- Codebase questions (query graph before grep): `.agents/shared/rules/graphify.md`
- New features / contract changes: `.agents/shared/rules/openspec-workflow.md`

## Conditional: Scopes

- `crates/walpad-core`: `.agents/shared/scopes/core.md`
- `crates/walpad-shell`, `crates/walpad-ui`: `.agents/shared/scopes/shell.md`
- `scripts/build_rootfs.sh`, `config/rootfs*`, device scripts: `.agents/shared/scopes/rootfs.md`

## Conditional: Memory

- Hardware, partitions, input, display, audio, modem: `.agents/shared/memory/hardware.md`
- Traps already paid for: `.agents/shared/memory/known-issues.md`
- Settled decisions: `.agents/shared/memory/decisions.md`
- Inactive legacy flows: `.agents/shared/memory/legacy.md`

## Conditional: Docs

- IPC contract: `docs/rust/CONTRACTS.md`; UI spec: `docs/rust/UI.md`; task cards:
  `docs/rust/TASKS.md`; flash runbook: `docs/rust/FLASH-RUNBOOK.md`; full history:
  `docs/99-status-log.md` (read sections, not the whole file)

## Daily History (Asia/Dhaka date, gitignored)

- `.agents/shared/history/YYYY-MM-DD/caveman.md` (state), `updates.md`, `decisions.md`,
  `plans.md`, `handoffs.md`; format: `.agents/shared/caveman.md`

## Do Not

- Put real content in `.claude/` or `.codex/` files
- Flash, erase or write partitions without the owner's explicit approval
- Store secrets or raw chat logs anywhere
