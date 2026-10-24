# wkit roadmap

`wkit` collapses the per-repo scripts in `~/work/radar/workspace` and `~/work/quasar` into one Rust
executable driven by `wkit.toml`. Justfiles end up as thin proxies (`fmt: wkit fmt`) or disappear.

## What the scripts do today

Survey of ~70 unique Justfiles and the scripts behind them (fpga, build caches and golden fixtures excluded).

| Script / recipe                                                                                                         | Copies      | Becomes                                |
|-------------------------------------------------------------------------------------------------------------------------|-------------|----------------------------------------|
| `scripts/version.sh` (≈10 variants: conan, cmake, cargo+lock, PKGBUILD, meta.lua, `version.h`, CalVer) + `wucc version` | 39 + 4      | `wkit bump`                            |
| `scripts/format.py` + `clang_tools.py`, `cargo +nightly fmt`, `find … clang-format -i`                                  | 35 + 12     | `wkit fmt`                             |
| `scripts/check.py` (pinned clang-format/clang-tidy via uv, tidy result cache)                                           | 35          | `wkit check`                           |
| `scripts/check_versions.py` (conanfile ⇔ meta.lua)                                                                      | 16          | built-in `versions` check              |
| `scripts/clean.sh` + `clean.ps1`, uv `[script]` clean recipes                                                           | 31 + 31 + 6 | `wkit clean`                           |
| `graph` recipe (`conan graph info --format=html`)                                                                       | 37          | `wkit graph`                           |
| `scripts/copy.sh` + `copy.ps1` (plugin → `<mms-bin>/plugins`)                                                           | 15 + 15     | `wkit deploy`                          |
| `scripts/build.sh`/`.ps1` (conan wrapper: `-C -t -o -d/-r/-m/-w -j -v`)                                                 | 3 + 3       | `wkit build`                           |
| `test` recipes: Catch2 binaries, `ctest`, `cargo nextest --profile ci`                                                  | 8           | `wkit test`                            |
| `package-arch` (git ls-files tarball, version mismatch check, makepkg)                                                  | 8           | `wkit package arch`                    |
| `package-deb` (cargo-deb or conan build + `cmake --install` + `package-deb.sh`)                                         | 8           | `wkit package deb`                     |
| `install`/`uninstall` (distro detection → apt/pacman, or `sudo install` to system paths)                                | 11          | `wkit install`, `wkit uninstall`       |
| `create`, `conan workspace build/create`                                                                                | 5           | `wkit create`, `build.conan.workspace` |
| `ci`, `ci-profile` (fmt check + clippy + test across feature/option matrices)                                           | 5           | `wkit ci`                              |
| `purge` (XDG runtime data)                                                                                              | 3 + 3       | `wkit purge`                           |
| `sync` (upload resolved Conan graph to a remote)                                                                        | 2           | `wkit conan sync`                      |
| `dbuild`, `build-prod`, armv7 podman builds (podman → docker fallback)                                                  | 5           | `wkit container`                       |
| `scripts/workspace.py` (init/restore/update/lock/status/add/exec/fmt/commit/stats)                                      | 1           | `wkit ws`                              |
| misc: `doc` (poxy), `coverage` (llvm-cov), `smoke.sh`, `check_linkage.py`, `qmllint`, `golden`, `vendor`, `sysinfo`     | 1–2 each    | see below                              |

On Windows, 60+ of these files exist only as `.ps1` twins of a `.sh` script. Built-in commands remove both.

## Workspace layout

| Crate           | Status | Contents                                                                                                                                                                                                                                                                       |
|-----------------|--------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `wkit_core`     | ✅     | cargo-style `Shell` + progress, `ProcessBuilder`, semver/CalVer bumping and flavors (full, pkgver, numeric), language/build-system/rustfmt-channel detection, gitignore-aware source walking, uv-pinned tools                                                                  |
| `wkit_manifest` | ✅     | `wkit.toml` schema, shorthand forms, unused-key warnings, validation, project discovery. Reference: [`reference.toml`](wkit_manifest/tests/fixtures/reference.toml)                                                                                                            |
| `wkit_version`  | ✅     | format-preserving slot splicing: conanfile class attribute, CMake `project(VERSION)`, Cargo via spanned TOML (`[workspace.package]`/`[package]` + local `Cargo.lock` packages at the old version), PKGBUILD `_source_version`/`pkgver`/`pkgrel=1`, lua/regex; mismatch reports |
| `wkit_fmt`      | ✅     | rustfmt (channel auto-detected), clang-format batches in parallel, qmlformat/black/stylua when configured, changed-file report, `--check`                                                                                                                                      |
| `wkit_check`    | M2     | clippy, clang-tidy (compile db discovery, gcc-only flag stripping, parallel, result cache), qmllint, MMS `meta.lua` validation, custom checks, summary table                                                                                                                   |
| `wkit_test`     | M2     | cargo/nextest/ctest/catch2 runners, JUnit aggregation                                                                                                                                                                                                                          |
| `wkit_build`    | M3     | conan (single and workspace), plain CMake (presets or `build/<type>`) and cargo drivers, build type from the Conan profile, conan/CMake/ninja output translated to cargo-style lines and a progress bar                                                                        |
| `wkit_package`  | M4     | arch (makepkg + git tarball), deb (own writer or cargo-deb), windows (CPack), apt/pacman/file installs                                                                                                                                                                         |
| `wkit_ws`       | M5     | multi-repo workspaces (`workspace.toml`/`workspace.lock` compatible): init/restore/update/lock/status/add/exec/fmt, parallel git with progress, commit/push                                                                                                                    |
| `wkit`          | ✅ M1  | the binary: clap CLI, command dispatch, `[scripts]`, `wkit-<name>` external subcommands, `self-update`, `init`, `clean`                                                                                                                                                        |

## Milestones

### M0: foundations ✅
- [x] Workspace with `wkit_core` and `wkit_manifest`
- [x] Full semver bump rules (prereleases release on bump, build metadata dropped), CalVer `YYYY.MM.DD[.N]`
- [x] PKGBUILD flavor: no dashes, numeric prereleases get a `pre` prefix so `vercmp` still sorts them first
- [x] Numeric flavor for CMake (`project(VERSION)` rejects prereleases)

### M1: the binary, bump, fmt, clean ✅
Enough to delete `version.sh`, `format.py` and `clean.sh`.
- [x] `wkit` crate. Global flags `-v/-q`, `--color`, `--manifest-path`, `-C <dir>`. Errors print as `error:` followed by `Caused by:`. Exit codes: a failed child's code is passed through, internal errors exit 101
- [x] `wkit bump [-M|-m|-p|-a <ver>] [--pre <id>] [--dry-run]`. With no flag it shows every file's version and flags mismatches. `--pre rc` starts or advances a prerelease (`1.2.3` → `1.2.4-rc.1` → `1.2.4-rc.2`). For calver, any bump means "next"
- [x] `wkit bump --commit [--tag]`: commit the version files as `v<version>` and tag `<version>`
- [x] `wkit fmt [--check] [-j N]` (alias `format`): `cargo fmt` or `cargo +nightly fmt`, then pinned clang-format via `uv tool run`
- [x] `wkit clean [--dry-run]`
- [x] `wkit run-script <name>` / `wkit x <name>` for `[scripts]`; unknown subcommands fall back to scripts, then to `wkit-<name>` binaries in PATH
- [x] `wkit self-update [--version <v>]`: compare with the crates.io sparse index (`index.crates.io/wk/it/wkit`). Prefer `cargo binstall wkit`, otherwise `cargo install wkit --locked --force`. Honor `project.wkit` requirements
- [x] `wkit init`: write `wkit.toml` from detection (languages, build system, version files found, test runners). The rustfmt channel stays auto-detected

### M2: check and test ✅
Enough to delete `check.py`, `clang_tools.py`, `check_versions.py` and `test.sh`.
- [x] `wkit check [--only <stage>] [--skip <stage>] [-j N] [--no-cache] [-v] [BUILD_DIR]`. Stages: `format`, `versions`, `clippy`, `clang-tidy`, `secrets`, `whitespace`, `pre-commit`, then custom checks. A failed stage exits 1
- [x] clang-tidy cache keyed by tool version, `.clang-tidy` digests, compile command and every included file (`-header-include-file`). Prune on run; skip files modified mid-run
- [x] Built-in check kinds beyond custom commands: `secrets` (`github_pat_`, tokens), `whitespace` (`git diff --check`), `pre-commit` (pre-commit `rev` matches `[tools]`)
- [x] `wkit test [--junit [DIR]] [--suite <name>] [-r|--release] [--build-type T] [--profile P] [-- args]`. Builds C/C++ with tests first (incremental), then merges every suite into `DIR/junit.xml`
  - nextest: `--profile ci`; without a `junit.path` in `.config/nextest.toml`, wkit supplies one via `--tool-config-file`
  - catch2: `--reporter JUnit::out=<file> --reporter console::out=-::colour-mode=ansi`, `--order rand --rng-seed $CI_PIPELINE_IID`
  - ctest: `--output-on-failure --output-junit <file>`
  - cargo: switch to nextest when `--junit` is requested and nextest is installed, otherwise fail with a hint
  - Without suites, infer them: Rust → nextest if installed, else cargo; C/C++ → ctest in the build dir
  - A suite that fails without writing JUnit still shows up as a failed testsuite
- [x] `QT_QPA_PLATFORM=offscreen` default for test env on headless CI (`$CI` set)

### M3: build, run, conan helpers ✅
- [x] `wkit build [-r|--release|--build-type T] [--profile P] [-t|--tests] [--offline] [-j N] [-- extra]`
  - The manifest never sets the build type: it comes from `--build-type`, then the Conan host profile (CMake presets for plain CMake), then Debug
  - The manifest never sets Conan options: the conanfile defaults apply. `--tests` turns on the conanfile's test option (`test`, `tests`, `with_tests`, … or `build.conan.test-option`)
  - Conan, CMake and ninja output becomes `Compiling`/`Configuring`/`Downloading` lines and a progress bar; compiler diagnostics pass through, `-v` shows the raw output, the full log goes to `build/wkit-build.log`
  - `wkit check` builds first when clang-tidy has no compile database
- [x] `wkit run [--bin <name>] [-- args]` with `default-run`, `[[bin]]` path templates, env, cwd. Builds first; bins without `path` go through `cargo run`
- [x] `wkit configure [-t]`: `conan install` + resolve the Conan CMake preset (`CMakeUserPresets.json` includes, preset matching the build type) + `cmake --preset`. In a workspace, packages other than the last are also built so dependents can find them
- [x] `wkit create`: `conan create` / `conan workspace create`, test package output translated like `build`
- [x] `wkit graph [-o <file>] [--open]`: `conan graph info . --format=html` → `docs/graph.html`
- [x] `wkit conan sync [--remote <r>] [--dry-run]`: upload the resolved graph from the local cache one package at a time (`[conan.sync] remote/check/force`, replaces `ConanRemoteSync.toml`). Fails on missing binaries; the root package is never uploaded
- [x] `compile_commands.json` symlink to the active build dir after `build`/`configure` (`build.link-compile-commands`, single-package projects; real files are left alone)
- [x] `wkit clean` also cleans every Conan workspace package (`conanws.yml`/`conanws.py`)

### M4: packaging and installation ✅
- [x] `wkit package arch [--set S] [-o DIR] [-- makepkg args]`: tarball from `git ls-files --cached --others --exclude-standard` (skipping package outputs), check versions against PKGBUILD, `makepkg --syncdeps --cleanbuild --clean --force`, `PKGDEST` support
  - Tarball names, `pkgver` and `pkgname` come from `makepkg --printsrcinfo`, so the PKGBUILD stays the only source. The tarball is removed after the build
  - Missing `depends`/`makedepends` are installed through `sudo pacman -S --needed --asdeps` up front, so the sudo prompt stays visible
- [x] `wkit package deb [--set S] [-o DIR]`: cargo-deb (`--no-build` after wkit's release build), or conan/CMake build + `cmake --install` + a declarative control file (`[package.deb]`) instead of `package-deb.sh`
  - The `.deb` is written by wkit (ar + gzip tarballs, root-owned, `/etc` files as conffiles); `dpkg-deb` is not needed. Maintainer scripts come from `packaging/debian/`
  - Prereleases use `~` (`1.2.3~rc.1`) so they sort before the release
- [x] `wkit package windows`: NSIS/WiX/zip from the install tree through CPack (`include(CPack)` in CMakeLists.txt)
- [x] `wkit install [--set <deployment>] [--files] [--dry-run]` / `wkit uninstall`: detect debian-like vs arch-like (`ID`, `ID_LIKE`), build the package, install with apt/pacman. Otherwise `[install]` file mappings for `sudo install`, preserving existing configs, `systemd-tmpfiles`, `systemctl daemon-reload`
- [x] Shared `[package]` settings: `name`, `exclude` (defaults drop `include/`, `lib/cmake`, `lib/pkgconfig`), extra `files`, `license`, and a `stage` hook that can patch or verify the staged tree (rpaths, forbidden libraries) with `WKIT_STAGE_DIR`/`WKIT_PACKAGE_*` set
- [x] Packages build in Release unless `--build-type` says otherwise. `--set` passes `-o &:<package.set-option>=<set>` to Conan; sets other than the conanfile default are packaged as `<name>-<set>`

### M5: workspace and extras ✅
- [x] `wkit smoke [BIN] [--runs N --interval S --timeout S --expect-exit]`: repeated launch stability test
  - A launch passes when it exits cleanly or is still alive at the timeout (then its process group gets SIGTERM, SIGKILL after 2s). `--expect-exit` fails survivors
  - Output goes to `build/smoke/run-N.log` (`target/` for Rust-only projects); failed runs print their tail. Cargo binaries are built, then launched directly rather than through `cargo run`
- [x] `wkit doctor`: uv, conan, cmake, ninja, nightly toolchain, cargo-nextest, cargo-deb, makepkg, pinned tools resolvable
  - Probes run in parallel and are required/recommended/optional depending on the project; a missing required tool exits 1. Outside a project everything is optional
- [x] `wkit info [--json]`: host triple, OS/distro, kernel, CPUs, and a project summary (version, languages, build system, bins, suites, formatters, planned checks, package formats)
- [x] Extra formatters/linters behind `[format.<name>]`: qmlformat (`.qmlformat.ini`), qmllint, black, stylua
  - `[format.qmlformat]`, `[format.black]`, `[format.stylua]` turn on by themselves when `.qmlformat.ini`, `[tool.black]` in `pyproject.toml` or `stylua.toml` exist. `qmllint` is a check stage (`[check.qmllint]`, opt-in)
  - Unpinned Qt tools prefer Qt 6 (`$QT_HOST_BINS`, `/usr/lib/qt6/bin`, …) over a Qt 5 `qmlformat` on `PATH`
- [x] MMS extension: `meta.lua` validation (uuid, dependency ranges), plugin version ⇔ conanfile
  - `mms` check stage, on when the tree has exactly one `meta.lua` (or `check.mms.meta` names it). UUIDs resolve through `local` variables and `mms.Uuid.from_string`; `mms.VersionRange.parse` and `{ name, version }` ranges must parse and match some version; the plugin must not depend on itself
  - The plugin version must equal `[version]`'s current version, else the conanfile's. `mms.app.version` is reported as a note
- [x] `wkit ws`: multi-repo workspace management, replacing `scripts/workspace.py`, `exec.py`, `fmt.py` and `commit.py`
  - `workspace.toml` (`version = 1`, ordered `[[project]] path/url/branch`) and `workspace.lock` (`[[revision]] path/commit`) are read and written in the same format, so the Python tooling keeps working side by side
  - `init` clones into `.name.partial` and moves it into place only when checked out; projects missing from the lock are cloned at their branch tip and locked. `restore` returns clean projects to the lock, `update` fast-forwards and reports ahead/diverged/dirty projects instead of touching them
  - `lock [--commit [--push]]` refuses dirty projects and refuses to push a lock that references unpublished commits; `update --commit` relocks and commits. `add <url> [path] [-b B]` derives the path from the URL layout, keeps related projects together in `workspace.toml` and updates `.gitignore`
  - `status`, `exec` (one argument runs through the shell), and `fmt [--commit [--push]]` (`wkit fmt` where there is a `wkit.toml`, else the project's `just fmt`/`format`)
  - `commit fmt|lock [--push]` commits pending project changes as `CHORE: Formatting`, or `workspace.lock` as `CHORE: Update lock` with the moved projects listed

## Cross-cutting rules
- Status goes to stderr in cargo style (`  Formatting`, `    Checking`, `     Bumping`, `    Finished`). Data goes to stdout. Progress bars are hidden when stderr is not a TTY or with `-q`
- Precedence: CLI flags > `WKIT_*` env > `wkit.toml` > built-in defaults
- Tools are pinned through `uv tool run --from <pkg>==<ver>`. Every pin has a default (`clang-format 23.1.3`, `clang-tidy 22.1.8`). `--offline` sets `UV_OFFLINE=1` and passes `-nr` to conan
- Every built-in is cross-platform: no `sh`/`ps1` pairs
- Unknown manifest keys warn instead of failing. `project.wkit` sets a minimum version for manifests that use newer keys
