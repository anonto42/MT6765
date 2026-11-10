# Shared Agent Workspace

Canonical Claude/Codex knowledge lives in `.agents/shared/`.

- `shared/agent-startup.md`: startup and work loop.
- `shared/caveman.md`: compressed state format.
- `shared/history/YYYY-MM-DD/`: daily working memory (gitignored, local).
- `shared/memory/`, `shared/rules/`, `shared/scopes/`: load only as needed.

Do not duplicate instructions under `.claude/` or `.codex/`: files there are one-line
`@.agents/shared/...` proxies.
