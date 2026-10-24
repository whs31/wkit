---
name: wkit
description: Standard wkit workflow for repositories with a wkit.toml. Use when formatting or auditing code, building, testing or running the project, checking or changing the project version, cleaning build outputs, running project scripts, or deciding whether to invoke clang-format, clang-tidy, cargo fmt, conan, cmake or version edits directly.
---

# wkit workflow
`wkit` is the canonical interface for common developer operations in a repository with a `wkit.toml`.
Read `wkit.toml` to learn what the project declares; `wkit info` summarizes it and `wkit doctor` lists the tools it needs.

## Formatting
```sh
wkit fmt
```

Use `wkit fmt` instead of invoking `clang-format`, `cargo fmt`, `qmlformat`, `black`, `stylua` or equivalent tools directly.
It uses the tool versions pinned in `[tools]`, so a different formatter from `PATH` would produce different output.
Run it after modifying source files covered by the formatter. Do not hand-format around its output.

If unrelated files were modified, keep the changes.

## Static checks
Run `wkit check` right after `wkit fmt`:
```sh
wkit fmt
wkit check
```

`wkit check` runs every enabled stage without modifying files: formatting, version consistency, clippy, clang-tidy, qmllint, secrets, whitespace, pre-commit pins and the project's `[[check.custom]]` checks.
Use it instead of invoking `clang-format --dry-run`, `clang-tidy`, `run-clang-tidy` or `cargo clippy` directly.

Select stages with `--only` or `--skip`:
```sh
wkit check --only clang-tidy
wkit check --skip clang-tidy
```

clang-tidy needs a configured build directory with `compile_commands.json`. If none exists, or the changed sources are not in it yet, run `wkit configure` or `wkit build` first. A specific build directory can be passed as an argument:
```sh
wkit check build/Release
```

Treat every reported finding in changed code as something to fix before the change is done:
- formatting issues mean `wkit fmt` was not run after the last edit;
- fix clang-tidy and clippy findings in the code rather than suppressing them;
- use `NOLINT` or `#[allow]` only with a specific check name and a comment explaining why the finding does not apply.

Findings in code unrelated to the current change should be reported, not fixed, unless the task asks for it.

## Build, test and run
```sh
wkit configure          # install dependencies and configure without compiling
wkit build              # Conan, CMake or Cargo, as `[build]` decides
wkit build --release    # or --build-type RelWithDebInfo
wkit build --tests      # also build the tests
wkit test               # build with tests and run every `[[test.suite]]`
wkit run -- ARGS        # build and run `project.default-run` or `--bin NAME`
```

Prefer these over raw `conan`, `cmake` or `cargo` invocations: they apply the profiles, settings, test option and build directories configured in `wkit.toml`.
Arguments after `--` are passed to the underlying tool (`build`, `configure`, `test`) or program (`run`).
Use `--offline` to resolve dependencies only from the local cache and `-j N` to limit parallel jobs.

## Version management
Show the version of every version file:
```sh
wkit bump
```

Increment versions:
```sh
wkit bump -p
wkit bump -m
wkit bump -M
```

Assign an explicit version or start a prerelease:
```sh
wkit bump -a X.Y.Z
wkit bump -a X.Y.Z-rc.1
wkit bump --pre rc
```

`--dry-run` shows what would change. Do not pass `--commit` or `--tag` unless asked to commit.
Do not manually change version fields listed in `[version]`; `wkit bump` keeps them synchronized and `wkit check` fails when they disagree.

## Other commands
- `wkit clean` removes build outputs; `--dry-run` lists them first.
- `wkit graph` renders the Conan dependency graph.
- `wkit conan sync` uploads the resolved Conan graph to the remote in `[conan.sync]`.
- `wkit package arch|deb|windows` and `wkit install` build packages and install system-wide. Only run them when the task asks for it.
- `wkit x` lists the project's `[scripts]`; `wkit x NAME ARGS` runs one.

## General rule
Prefer an existing `wkit` command or `[scripts]` entry over reproducing its implementation in shell commands.
Do not assume scripts, test suites or binaries other than the ones declared in the current `wkit.toml`.
