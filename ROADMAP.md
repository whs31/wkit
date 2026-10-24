# wkit roadmap

`wkit` collapses the per-repo scripts in `~/work/radar/workspace` and `~/work/quasar` into one Rust
executable driven by `wkit.toml`. Justfiles end up as thin proxies (`fmt: wkit fmt`) or disappear.

## What the scripts do today

Survey of ~70 unique Justfiles and the scripts behind them (fpga, build caches and golden fixtures excluded).

| Script / recipe | Copies | Becomes |
| --- | --- | --- |
| `scripts/version.sh` (≈10 variants: conan, cmake, cargo+lock, PKGBUILD, meta.lua, `version.h`, CalVer) + `wucc version` | 39 + 4 | `wkit bump` |
| `scripts/format.py` + `clang_tools.py`, `cargo +nightly fmt`, `find … clang-format -i` | 35 + 12 | `wkit fmt` |
| `scripts/check.py` (pinned clang-format/clang-tidy via uv, tidy result cache) | 35 | `wkit check` |
| `scripts/check_versions.py` (conanfile ⇔ meta.lua) | 16 | built-in `versions` check |
| `scripts/clean.sh` + `clean.ps1`, uv `[script]` clean recipes | 31 + 31 + 6 | `wkit clean` |
| `graph` recipe (`conan graph info --format=html`) | 37 | `wkit graph` |
| `scripts/copy.sh` + `copy.ps1` (plugin → `<mms-bin>/plugins`) | 15 + 15 | `wkit deploy` |
| `scripts/build.sh`/`.ps1` (conan wrapper: `-C -t -o -d/-r/-m/-w -j -v`) | 3 + 3 | `wkit build` |
| `test` recipes: Catch2 binaries, `ctest`, `cargo nextest --profile ci` | 8 | `wkit test` |
| `package-arch` (git ls-files tarball, version mismatch check, makepkg) | 8 | `wkit package arch` |
| `package-deb` (cargo-deb or conan build + `cmake --install` + `package-deb.sh`) | 8 | `wkit package deb` |
| `install`/`uninstall` (distro detection → apt/pacman, or `sudo install` to system paths) | 11 | `wkit install`, `wkit uninstall` |
| `create`, `conan workspace build/create` | 5 | `wkit create`, `build.conan.workspace` |
| `ci`, `ci-profile` (fmt check + clippy + test across feature/option matrices) | 5 | `wkit ci` |
| `purge` (XDG runtime data) | 3 + 3 | `wkit purge` |
| `sync` (upload resolved Conan graph to a remote) | 2 | `wkit conan sync` |
| `dbuild`, `build-prod`, armv7 podman builds (podman → docker fallback) | 5 | `wkit container` |
| `scripts/workspace.py` (init/restore/update/lock/status/add/exec/fmt/commit/stats) | 1 | `wkit ws` |
| misc: `doc` (poxy), `coverage` (llvm-cov), `smoke.sh`, `check_linkage.py`, `qmllint`, `golden`, `vendor`, `sysinfo` | 1–2 each | see below |

On Windows, 60+ of these files exist only as `.ps1` twins of a `.sh` script. Built-in commands remove both.

## Workspace layout

| Crate | Status | Contents |
| --- | --- | --- |
| `wkit_core` | ✅ | cargo-style `Shell` + progress, `ProcessBuilder`, semver/CalVer bumping and flavors (full, pkgver, numeric), language/build-system/rustfmt-channel detection, gitignore-aware source walking, uv-pinned tools |
| `wkit_manifest` | ✅ | `wkit.toml` schema, shorthand forms, unused-key warnings, validation, project discovery. Reference: [`reference.toml`](wkit_manifest/tests/fixtures/reference.toml) |
| `wkit_version` | M1 | per-kind editors that preserve formatting: conanfile class attribute, CMake `project(VERSION)`, Cargo via `toml_edit` (`[workspace.package]`/`[package]` + workspace members in `Cargo.lock`), PKGBUILD `pkgver`/`_source_version`/`pkgrel=1`, lua/regex |
| `wkit_fmt` | M1 | rustfmt (channel auto-detected), clang-format batches in parallel, changed-file report, `--check` |
| `wkit_check` | M2 | clippy, clang-tidy (compile db discovery, gcc-only flag stripping, parallel, result cache), custom checks, summary table |
| `wkit_test` | M2 | cargo/nextest/ctest/catch2 runners, JUnit aggregation |
| `wkit_build` | M3 | conan/cargo/cmake drivers, configure + preset resolution, `[[bin]]` resolution |
| `wkit_package` | M4 | arch/deb/windows packaging, install/uninstall |
| `wkit_ws` | M5 | multi-repo workspace management (`workspace.toml`/`workspace.lock` compatible) |
| `wkit` | M1 | the binary: clap CLI, command dispatch, `[scripts]`, `wkit-<name>` external subcommands, `self-update` |

## Milestones

### M0: foundations ✅
- [x] Workspace with `wkit_core` and `wkit_manifest`
- [x] Full semver bump rules (prereleases release on bump, build metadata dropped), CalVer `YYYY.MM.DD[.N]`
- [x] PKGBUILD flavor: no dashes, numeric prereleases get a `pre` prefix so `vercmp` still sorts them first
- [x] Numeric flavor for CMake (`project(VERSION)` rejects prereleases)

### M1: the binary, bump, fmt, clean
Enough to delete `version.sh`, `format.py` and `clean.sh`.
- [ ] `wkit` crate. Global flags `-v/-q`, `--color`, `--manifest-path`, `-C <dir>`. Errors print as `error:` followed by `Caused by:`. Exit codes: a failed child's code is passed through, internal errors exit 101
- [ ] `wkit bump [-M|-m|-p|-a <ver>] [--pre <id>] [--dry-run]`. With no flag it shows every file's version and flags mismatches. `--pre rc` starts or advances a prerelease (`1.2.3` → `1.2.4-rc.1` → `1.2.4-rc.2`). For calver, any bump means "next"
- [ ] `wkit bump --commit [--tag]`: commit the version files and tag `v<version>`
- [ ] `wkit fmt [--check]` (alias `format`): `cargo fmt` or `cargo +nightly fmt`, then pinned clang-format via `uv tool run`
- [ ] `wkit clean [--dry-run]`
- [ ] `wkit run-script <name>` / `wkit x <name>` for `[scripts]`; unknown subcommands fall back to scripts, then to `wkit-<name>` binaries in PATH
- [ ] `wkit self-update [--version <v>]`: compare with the crates.io sparse index (`index.crates.io/wk/it/wkit`). Prefer `cargo binstall wkit`, otherwise `cargo install wkit --locked --force`. Honor `project.wkit` requirements
- [ ] `wkit init`: write `wkit.toml` from detection (languages, build system, version files found, rustfmt channel, test runners)

### M2: check and test
Enough to delete `check.py`, `clang_tools.py`, `check_versions.py` and `test.sh`.
- [ ] `wkit check [--only <stage>] [--skip <stage>] [-j N] [--no-cache] [-v] [BUILD_DIR]`. Stages: `format`, `versions`, `clippy`, `clang-tidy`, then custom checks
- [ ] clang-tidy cache keyed by tool version, `.clang-tidy` digests, compile command and every included file (`-header-include-file`). Prune on run; skip files modified mid-run
- [ ] Built-in check kinds beyond custom commands: `secrets` (`github_pat_`, tokens), `whitespace` (`git diff --check`), `pre-commit-pins` (pre-commit `rev` matches `[tools]`)
- [ ] `wkit test [--junit [DIR]] [--suite <name>] [--release] [-- args]`
  - nextest: `--profile ci` writes JUnit natively
  - catch2: `--reporter JUnit::out=<file> --reporter console::out=-::colour-mode=ansi`, `--order rand --rng-seed $CI_PIPELINE_IID`
  - ctest: `--output-on-failure --output-junit <file>`
  - cargo: switch to nextest when `--junit` is requested and nextest is installed, otherwise fail with a hint
  - Without suites, infer them: Rust → nextest if installed, else cargo; C/C++ → ctest in the build dir
- [ ] `QT_QPA_PLATFORM=offscreen` default for test env on headless CI (`$CI` set)

### M3: build, run, conan helpers
- [ ] `wkit build [-r|--release|--build-type T] [-C|--configure] [-t|--tests] [--offline] [-j N] [-- extra]`, mirroring `build.sh` flags
- [ ] `wkit run [--bin <name>] [-- args]` with `default-run`, `[[bin]]` path templates, env, cwd
- [ ] `wkit configure`: `conan install` + resolve the Conan CMake preset (`CMakeUserPresets.json` includes, newest `conan-<type>`) + `cmake --preset`
- [ ] `wkit create`: `conan create` / `conan workspace create`
- [ ] `wkit graph [--open]`: `conan graph info . --format=html` → `docs/graph.html`
- [ ] `wkit conan sync [--dry-run]`: upload the resolved graph from the local cache (`[conan.sync] remote`, replaces `ConanRemoteSync.toml`)
- [ ] `compile_commands.json` symlink to the active build dir (several repos keep one by hand)

### M4: packaging and installation
- [ ] `wkit package arch`: tarball from `git ls-files --cached --others --exclude-standard` (skipping package outputs), check versions against PKGBUILD, `makepkg --syncdeps --cleanbuild --force`, `PKGDEST` support
- [ ] `wkit package deb`: cargo-deb, or conan build + `cmake --install` + a declarative control file (`[package.deb]`) instead of `package-deb.sh`
- [ ] `wkit package windows`: NSIS/MSI from an install tree (corona, ics2)
- [ ] `wkit install [--set <deployment>]` / `wkit uninstall`: detect debian-like vs arch-like (`ID`, `ID_LIKE`), build the package, install with apt/pacman. Otherwise `[install]` file mappings for `sudo install`, preserving existing configs, `systemd-tmpfiles`, `systemctl daemon-reload`
- [ ] `wkit purge [--settings|--keep-license]`: `[purge]` groups of XDG paths
- [ ] `wkit container <recipe>`: podman with a docker fallback, `--platform`, `--output type=local`, qemu binfmt enabling for cross targets

### M5: workspace and extras
- [ ] `wkit ws init|restore|update|lock|status|add|exec|reset|commit|stats` over `workspace.toml`/`workspace.lock`. `wkit ws fmt` runs `wkit fmt` per project instead of searching for Justfiles
- [ ] `wkit ws submodules update` for submodule-based aggregates (quasar/simulator)
- [ ] `wkit ci [--profile <name>]`: run fmt check, check, test and release build locally over `[ci]` matrices (cargo feature sets, conan option sets as in rcommon's Qt × adapters)
- [ ] `wkit deploy <dir>`: copy built plugin libraries (`.so`/`.dll`/`.dylib`) into `<dir>/plugins`
- [ ] `wkit doc`: poxy for C++, `cargo doc` for Rust
- [ ] `wkit coverage`: `cargo llvm-cov nextest --lcov`
- [ ] `wkit smoke <bin> [--runs N --interval S --timeout S]`: repeated launch stability test
- [ ] `wkit linkage`: forbidden static symbols in an install tree, `ldd` expectations per option set
- [ ] `wkit doctor`: uv, conan, cmake, ninja, nightly toolchain, cargo-nextest, cargo-deb, makepkg, pinned tools resolvable
- [ ] `wkit hooks install`: git pre-commit hook running `wkit fmt --check` (replaces `.pre-commit-config.yaml` pin duplication)
- [ ] `wkit info`: host triple, OS/distro, project summary (`sysinfo` recipes)
- [ ] `wkit gen embed|hexdump|json2cpp|yaml2cpp|export-header`: the old `wucc` generators
- [ ] Extra formatters/linters behind `[format.<name>]`: qmlformat (`.qmlformat.ini`), qmllint, black, stylua
- [ ] `wkit new <template>`: absorb `create-mms-project`
- [ ] MMS extension: `meta.lua` validation (uuid, dependency ranges), plugin version ⇔ conanfile

## Cross-cutting rules
- Status goes to stderr in cargo style (`  Formatting`, `    Checking`, `     Bumping`, `    Finished`). Data goes to stdout. Progress bars are hidden when stderr is not a TTY or with `-q`
- Precedence: CLI flags > `WKIT_*` env > `wkit.toml` > built-in defaults
- Tools are pinned through `uv tool run --from <pkg>==<ver>`. Every pin has a default (`clang-format 23.1.3`, `clang-tidy 22.1.8`). `--offline` sets `UV_OFFLINE=1` and passes `-nr` to conan
- Every built-in is cross-platform: no `sh`/`ps1` pairs
- Unknown manifest keys warn instead of failing. `project.wkit` sets a minimum version for manifests that use newer keys
