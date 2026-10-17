# netlimit

Query the graphify knowledge graph before grepping unfamiliar code. The graph is local and gitignored. A rebuild is AST-only and costs no model tokens.

This applies to OpenCode, Claude Code, Grok, and every subagent that explores code.

## Graphs

- **Rust** (`rest of repo`): `graphify query --graph graphify-out/graph.json "<question>"`
  `graphify path` and `graphify explain` take the same `--graph`. Update with `graphify update .` from `.`.

If `graph.json` is missing, search the code directly. If `GRAPH_REPORT.md` names a commit other than `git rev-parse --short HEAD`, update that package before relying on the graph.

## Rebuild

Hooks live in `.githooks/`. A clone rebuilds automatically after `scripts/install-graphify-hooks.sh` (`git config core.hooksPath .githooks`). After commit, checkout, merge, and rebase, only the package that changed is updated, in the background. Log: `~/.cache/graphify-rebuild.log`. Skip one commit with `GRAPHIFY_SKIP_HOOK=1`. The hook does not run mid-session.
