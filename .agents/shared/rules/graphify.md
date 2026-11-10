## graphify

This repo has a knowledge graph at graphify-out/ (Rust crates, C tools, scripts, config, docs;
the kernel tree, vendor blobs and build output are excluded via `.graphifyignore`).

- For codebase questions run `graphify query "<question>"` first; `graphify path "<A>" "<B>"` for
  relationships; `graphify explain "<concept>"` for one concept. These return a small subgraph,
  usually cheaper than grep output or GRAPH_REPORT.md.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review.
- Dirty graphify-out/ files are expected; they are not a reason to skip graphify.
- After modifying code run `graphify update .` (AST only, no API cost).
