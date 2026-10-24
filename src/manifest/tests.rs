use std::path::{Path, PathBuf};

use super::{
  Command, FileMode, MANIFEST_FILE, Manifest, PackageSection, Project, TestRunner, VersionFileKind,
  WindowsGenerator,
};
use crate::{
  Language,
  detect::{BuildSystem, RustfmtChannel},
  tool::{Tool, ToolSource},
  version::{Flavor, Scheme},
};

fn fixture(name: &str) -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("tests/fixtures")
    .join(name)
}

fn problems(source: &str) -> Vec<String> {
  let err = Manifest::parse(source).unwrap_err().to_string();
  assert!(err.starts_with("invalid manifest:"), "{err}");
  err
    .lines()
    .skip(1)
    .map(|line| line.trim_start_matches("  - ").to_owned())
    .collect()
}

#[test]
fn reference_manifest_is_complete_and_valid() {
  let m = Manifest::load(&fixture("reference.toml")).unwrap();
  assert_eq!(m.warnings, Vec::<String>::new());

  assert_eq!(
    m.languages().iter().copied().collect::<Vec<_>>(),
    [Language::Cpp, Language::Rust]
  );
  assert_eq!(m.default_bin().unwrap().name, "mms");

  let version = m.version.as_ref().unwrap();
  assert_eq!(version.scheme, Scheme::SemVer);
  assert_eq!(version.source_file().unwrap().kind, VersionFileKind::Conan);
  let kinds: Vec<_> = version.files.iter().map(|f| (f.kind, f.flavor)).collect();
  assert_eq!(
    kinds,
    [
      (VersionFileKind::Conan, Flavor::Full),
      (VersionFileKind::CMake, Flavor::Numeric),
      (VersionFileKind::Cargo, Flavor::Full),
      (VersionFileKind::Pkgbuild, Flavor::PkgVer),
      (VersionFileKind::Lua, Flavor::Full),
      (VersionFileKind::Regex, Flavor::Full),
      (VersionFileKind::Regex, Flavor::Numeric),
    ]
  );

  assert_eq!(m.format.rust.channel, Some(RustfmtChannel::Nightly));
  assert!(m.format.cpp.extensions.contains(&"cu".to_owned()));
  assert!(m.clippy_enabled() && m.clang_tidy_enabled());
  assert!(!m.check.whitespace && m.check.secrets);
  assert_eq!(m.format.qmlformat.args, ["--normalize"]);
  assert_eq!(m.format.black.include, ["/scripts/"]);
  assert_eq!(m.format.stylua.enabled, Some(false));
  assert!(m.check.qmllint.enabled && m.check.mms.enabled == Some(true));
  assert_eq!(
    m.check.mms.meta.as_deref(),
    Some(Path::new("src/plugin/meta.lua"))
  );
  assert_eq!(
    m.check.custom[0].run,
    Command::Argv(vec!["python3".into(), "scripts/check_meta.py".into()])
  );

  assert_eq!(m.build.system, Some(BuildSystem::Conan));
  assert_eq!(m.build.conan.test_option.as_deref(), Some("test"));
  assert!(m.build.link_compile_commands);
  assert_eq!(m.conan.sync.remote.as_deref(), Some("radar"));
  assert!(m.conan.sync.check && !m.conan.sync.force);
  assert_eq!(
    m.build.conan.setting_args(),
    ["-s", "compiler.cppstd=gnu23"]
  );

  let runners: Vec<_> = m.test.suites.iter().map(|s| s.runner).collect();
  assert_eq!(
    runners,
    [TestRunner::Nextest, TestRunner::Catch2, TestRunner::Ctest]
  );
  assert_eq!(
    m.scripts["golden"].description.as_deref(),
    Some("Regenerate golden files")
  );
  assert!(matches!(m.scripts["graph"].run, Command::Shell(_)));

  assert_eq!(m.package.set_option.as_deref(), Some("deployment_set"));
  assert_eq!(m.package.files[0].mode, Some(FileMode(0o755)));
  let deb = m.package.deb.as_ref().unwrap();
  assert_eq!(
    deb.relations()[0].1,
    ["libc6 (>= 2.28)", "rmms-qt6 (>= 6.11.1)"]
  );
  assert_eq!(m.package.windows.generator, WindowsGenerator::Nsis);
  assert!(m.install.files[1].config && m.install.files[2].is_glob());
  assert_eq!(m.install.services.len(), 1);
}

#[test]
fn minimal_manifest_gets_language_driven_defaults() {
  let m = Manifest::parse("[project]\nlanguages = [\"c++\"]\n").unwrap();
  assert!(m.cpp_format_enabled() && m.clang_tidy_enabled());
  assert!(!m.rust_format_enabled() && !m.clippy_enabled());
  assert!(m.check.format && m.check.versions && m.check.secrets);
  assert!(m.check.whitespace && m.check.pre_commit);
  assert_eq!(m.test.junit_dir(), std::path::Path::new("build/junit"));
  assert_eq!(m.build.conan.test_option, None);
  assert!(m.build.link_compile_commands && m.conan.sync.remote.is_none());
  assert!(m.version.is_none() && m.bins.is_empty() && m.default_bin().is_none());
  assert!(m.package.deb.is_none() && m.install.is_empty());
  assert_eq!(m.package.exclude, PackageSection::DEFAULT_EXCLUDE);
  let clean = m.clean.effective_paths(m.languages());
  assert!(clean.contains(&"build".to_owned()) && !clean.contains(&"target".to_owned()));
}

#[test]
fn unknown_keys_become_warnings() {
  let parsed = Manifest::parse(
    r#"
    [project]
    languages = ["rust"]
    colour = "red"

    [check.clipy]
    enabled = true
    "#,
  )
  .unwrap();
  assert_eq!(
    parsed.warnings,
    [
      "unused manifest key: check.clipy",
      "unused manifest key: project.colour"
    ]
  );
}

#[test]
fn parse_errors_point_at_the_source() {
  let err = Manifest::parse("[project]\nlanguages = [\"cobol\"]\n").unwrap_err();
  assert_eq!(err.to_string(), "failed to parse manifest");
  let rendered = err.root_cause().to_string();
  assert!(rendered.contains("line 2"), "{rendered}");
  assert!(rendered.contains("unknown variant `cobol`"), "{rendered}");
}

#[test]
fn validation_collects_every_problem() {
  let found = problems(
    r#"
    [project]
    languages = []
    default-run = "ghost"

    [version]
    scheme = "calver"
    source = "missing.py"
    files = ["Cargo.toml", "Cargo.toml"]

    [check.clang-tidy]
    enabled = true

    [build.conan]
    settings = { build_type = "Release" }

    [[bin]]
    name = "a"
    [[bin]]
    name = "a"

    [[test.suite]]
    runner = "catch2"
    profile = "ci"

    [[check.custom]]
    name = "x"
    run = "true"
    [[check.custom]]
    name = "x"
    run = "true"
    [[check.custom]]
    name = "clippy"
    run = "true"
    [[check.custom]]
    name = "mms"
    run = "true"

    [package]
    set-option = ""
    files = [{ from = "a/*.so", to = "/usr/lib", config = true }]

    [install]
    prefix = "opt/x"
    files = [{ from = "a", to = "usr/bin/a" }]
    "#,
  );
  let expected = [
    "`project.languages` must list",
    "duplicate `[[bin]]` named `a`",
    "`project.default-run` refers to `ghost`",
    "`Cargo.toml` is listed twice",
    "cannot hold a calendar version",
    "`version.source` (`missing.py`)",
    "`check.clang-tidy` is enabled",
    "must not set `build_type`",
    "uses catch2, which does not fit",
    "`profile` only applies to nextest",
    "needs a `binary`",
    "duplicate custom check `x`",
    "clashes with the built-in check",
    "`package.set-option` must not be empty",
    "so `to` must end with `/`",
    "`config` only applies to `[install] files`",
    "`install.prefix` must be an absolute path",
    "`usr/bin/a` must be an absolute path",
  ];
  for needle in expected {
    assert!(
      found.iter().any(|p| p.contains(needle)),
      "missing `{needle}` in {found:#?}"
    );
  }
}

#[test]
fn invalid_manifest_lists_problems_in_display() {
  let err = Manifest::parse("[project]\nlanguages = []\n").unwrap_err();
  assert_eq!(
    err.to_string(),
    "invalid manifest:\n  - `project.languages` must list at least one of \"c\", \"cpp\" or \"rust\""
  );
}

#[test]
fn project_is_discovered_from_nested_directories() {
  let dir = tempfile::tempdir().unwrap();
  std::fs::write(
    dir.path().join(MANIFEST_FILE),
    "[project]\nlanguages = [\"rust\"]\n",
  )
  .unwrap();
  let nested = dir.path().join("crates/a/src");
  std::fs::create_dir_all(&nested).unwrap();

  let project = Project::discover(&nested).unwrap().unwrap();
  assert_eq!(project.root, dir.path());
  assert_eq!(
    project.name(),
    dir.path().file_name().unwrap().to_string_lossy()
  );
  assert_eq!(
    project.tool("clang-format"),
    Tool::pinned("clang-format", "23.1.3")
  );
  assert_eq!(project.tool("cmake").source, ToolSource::System);

  let empty = tempfile::tempdir().unwrap();
  assert!(Project::discover(empty.path()).unwrap().is_none());
}

#[test]
fn wkit_requirement_is_enforced() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join(MANIFEST_FILE);
  std::fs::write(
    &path,
    "[project]\nlanguages = [\"rust\"]\nwkit = \">=0.3\"\n",
  )
  .unwrap();
  let project = Project::load(&path).unwrap();
  assert!(
    project
      .ensure_compatible(&semver::Version::new(0, 3, 1))
      .is_ok()
  );
  let err = project
    .ensure_compatible(&semver::Version::new(0, 2, 0))
    .unwrap_err();
  assert!(err.to_string().contains("run `wkit self-update`"), "{err}");
}

#[test]
fn repository_manifest_is_valid() {
  let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("wkit.toml");
  let manifest = Manifest::load(&root).unwrap();
  assert!(manifest.warnings.is_empty(), "{:?}", manifest.warnings);
}
