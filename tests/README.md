# Example projects

Mock projects that wkit is tested against (`wkit/tests/examples.rs`). They are small but real:
every Conan graph resolves against ConanCenter, every project builds, and its tests pass.
Keep versions consistent and sources formatted (`wkit -C tests/<name> fmt --check`).

| Project | Shape | Exercises |
| --- | --- | --- |
| `conan-lib` | Conan C++ library (spdlog; Catch2 under `test`) + `test_package` | conanfile, CMake, PKGBUILD with `_source_version`, regex `version.h`, catch2 + ctest suites, passes every `wkit check` stage |
| `conan-app` | Conan C++ application (CLI11, nlohmann_json, zlib; GoogleTest) | prerelease `1.8.0-rc.2`, MMS `meta.lua` (`mms` check), QML asset (qmlformat, qmllint), `[[bin]]`, custom checks, scripts |
| `conan-workspace` | `conanws.yml` with `radar-app` depending on `radar-core` | nested version files, `build.conan.workspace` |
| `mixed` | Conan C++ library + Rust workspace sharing one version | C++ and Rust together, `Cargo.lock` |
| `calver` | Conan C++ application (fmt) | `YYYY.MM.DD.N` in conanfile, CMake and PKGBUILD |
| `cargo-workspace` | Cargo workspace (anyhow) with `[workspace.package]` | nightly rustfmt, nextest `ci` profile, `Cargo.lock`, PKGBUILD |
| `cargo-crate` | Single Cargo package, several binaries | stable rustfmt, `default-run`, prerelease `3.0.0-beta.1` |
| `cmake-c` | Plain CMake C project with presets | no Conan, ctest |
