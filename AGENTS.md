# netlimit

Query the graph below before grepping unfamiliar code.

## Graphs

- **Rust** (`rest of repo`): `graphify query --graph graphify-out/graph.json "<question>"`
  Update with `graphify update .` from `.`.

## Rebuild

Hooks live in `.githooks/`. A clone rebuilds automatically after `scripts/install-graphify-hooks.sh` (`git config core.hooksPath .githooks`).
