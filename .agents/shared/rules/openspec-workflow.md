# OpenSpec Workflow

- New features, IPC/contract changes, cross-module work and device bring-ups with a plan go
  through `openspec/changes/<id>/` (proposal.md, design.md when needed, tasks.md, spec deltas)
  before implementation: `openspec new change <id>`, `openspec validate <id>`,
  `openspec list`, `openspec show <id>`; after shipping `openspec archive <id>`.
- Small fixes (one bug, one script tweak) do not need a change.
- Proposals state: who asked (owner request + date), what is out of scope, device risk
  (partitions/flash: none or what), secrets impact, how it is verified on the tablet.
- Project-wide constraints are in `openspec/config.yaml`; do not repeat code facts there.
