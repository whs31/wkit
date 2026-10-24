# AGENTS.md

`wkit` is one Rust CLI that replaces per-repo Justfile scripts. Usage: [README.md](README.md).

- A single binary crate. `src/cli.rs` and `src/commands/` are the CLI layer; domain modules: `manifest` (`wkit.toml` schema), `version`, `format`, `check`, `testing`, `build`, `package`, `ws` (multi-repo workspaces), plus helpers (`shell`, `process`, `fs`, `tool`, `detect`).
- Mock projects in `tests/` back the integration tests; keep them buildable and consistent.
- When the manifest schema changes, update `tests/fixtures/reference.toml` and the tests too.
- Format with `just fmt`. Before finishing, run `just lint` (pedantic) and `just test`. MSRV is 1.88.
- No extensive comments: express intent with names and tests. No doc-comment essays.
- Errors are `anyhow`; exit codes travel as `crate::Failure`, help text through `crate::hint`. User-facing output goes through `Shell` (cargo style).
- Prefer well-known crates (`fs-err`, `lazy-regex`, `rayon`, `derive_more`, `smart-default`) over hand-rolled helpers.
