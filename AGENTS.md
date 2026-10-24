# AGENTS.md

`wkit` is one Rust CLI that replaces per-repo Justfile scripts. Plan: [ROADMAP.md](ROADMAP.md).

- Crates live in `crates/`: `wkit_core` (shared primitives), `wkit_manifest` (`wkit.toml` schema).
- When the manifest schema changes, update `crates/wkit_manifest/tests/fixtures/reference.toml` and the tests too.
- Format with `just fmt`. Before finishing, run `just lint` (pedantic) and `just test`. MSRV is 1.88.
- No extensive comments: express intent with names and tests. No doc-comment essays.
- Libraries use `thiserror`. User-facing output goes through `wkit_core::Shell` (cargo style).
