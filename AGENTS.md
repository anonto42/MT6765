# Codex Loader (WalPad 8G)

Source of truth: `.agents/shared/`.

Read before working:

1. `.agents/shared/agent-startup.md` (load order, hard rules, work loop, daily history)
2. `.agents/shared/memory/MEMORY.md`
3. `.agents/shared/rules/README.md`, then only the matching rule files

Other shared knowledge, loaded only when relevant: `.agents/shared/memory/` (hardware,
known issues, decisions, legacy), `.agents/shared/scopes/` (core, shell, rootfs),
`docs/rust/` (contracts, UI spec, tasks, flash runbook).

Do not create `memory/`, `rules/`, `skills/`, `plans/`, `changes/` or `context/` under `.codex/`
or `.claude/`: every file there is a single-line `@.agents/shared/...` proxy.

## Codebase Graph

@.agents/shared/rules/graphify.md
