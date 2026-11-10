# Agent Startup (WalPad 8G)

Compact shared loader for Claude and Codex. Asia/Dhaka date.

## Load

1. `.agents/shared/memory/MEMORY.md` (always)
2. Today's `.agents/shared/history/YYYY-MM-DD/caveman.md`; create it from the format in
   `.agents/shared/caveman.md` if missing and work will continue here
3. `.agents/shared/rules/README.md`, then only the rule files the task needs
4. A scope from `.agents/shared/scopes/` when the work is inside one area

## Hard Rules

- Never flash, erase or write a partition/block device without the owner's explicit approval in
  the current conversation; never run `mtk.py`, `flash_os.sh`, `flash_partition.sh`,
  `fastboot_py.py` on your own. Details: `rules/device-safety.md`.
- Never edit `config/boot/*`, `tools/propshim/`, `scripts/mtk_stack_up.sh` or the CONNSYS
  (`stack_up`) section of `walpad-wifi`.
- No secret (Wi-Fi/hotspot passphrase, SIM PIN, IMEI, tunnel token) in logs, argv, fixtures,
  commits or chat. Backups of NV partitions stay under the gitignored `firmware/`.
- Every hardware read is Option/Result; unavailable is labelled, never guessed. Every blocking
  operation has a timeout.
- `scripts/check.sh` must print ALL CHECKS PASSED before a commit. Preserve unrelated worktree
  changes; no reset/force-push on main.
- Non-trivial features or cross-module changes go through an OpenSpec change first
  (`rules/openspec-workflow.md`).

## Work Loop

- Check the dirty worktree; identify the area: core, shell, ui, proto, rootfs scripts, C tools,
  device bring-up, docs.
- For codebase questions query graphify before grepping (`rules/graphify.md`).
- Device work goes over SSH (`rules/device-access.md`): deploy binaries with `scripts/deploy.sh`,
  test, then commit the source so the next image build includes it.
- After meaningful work append to today's `updates.md` and rewrite today's `caveman.md`
  (STATE / TODAY / NEXT / HANDOFFS / RISKS / LOAD). Record decisions in `decisions.md`,
  agent splits in `handoffs.md`. Keep files under 200 lines.
- Durable lessons (a fact a future session would otherwise rediscover) go to
  `memory/known-issues.md` or `memory/decisions.md`, not only to the daily history.
