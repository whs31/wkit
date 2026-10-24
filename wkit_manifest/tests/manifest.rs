use std::path::{Path, PathBuf};

use wkit_core::{
  Language,
  detect::{BuildSystem, RustfmtChannel},
  tool::{Tool, ToolSource},
  version::{Flavor, Scheme},
};
use wkit_manifest::{
  BuildType, Command, MANIFEST_FILE, Manifest, ManifestError, Project, TestRunner, VersionFileKind,
};

fn fixture(name: &str) -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("tests/fixtures")
    .join(name)
}

fn problems(source: &str) -> Vec<String> {
  match Manifest::parse(source) {
    Err(ManifestError::Invalid { problems, .. }) => problems,
    other => panic!("expected validation problems, got {other:?}"),
  }
}

#[test]
fn reference_manifest_is_complete_and_valid() {
  let parsed = Manifest::load(&fixture("reference.toml")).unwrap();
  assert_eq!(parsed.warnings, Vec::<String>::new());
  let m = parsed.manifest;

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
  assert_eq!(
    m.check.custom[0].run,
    Command::Argv(vec!["python3".into(), "scripts/check_meta.py".into()])
  );

  assert_eq!(m.build.system, Some(BuildSystem::Conan));
  assert_eq!(m.build.build_type, BuildType::RelWithDebInfo);
  assert_eq!(
    m.build.conan.option_args(),
    ["-o", "qt/*:shared=True", "-o", "&:with_map=False"]
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
}

#[test]
fn minimal_manifest_gets_language_driven_defaults() {
  let m = Manifest::parse("[project]\nlanguages = [\"c++\"]\n")
    .unwrap()
    .manifest;
  assert!(m.cpp_format_enabled() && m.clang_tidy_enabled());
  assert!(!m.rust_format_enabled() && !m.clippy_enabled());
  assert!(m.check.format && m.check.versions);
  assert_eq!(m.build.build_type, BuildType::Debug);
  assert!(m.version.is_none() && m.bins.is_empty() && m.default_bin().is_none());
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
  let ManifestError::Parse { source, .. } = &err else {
    panic!("{err:?}");
  };
  let rendered = source.to_string();
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
    "uses catch2, which does not fit",
    "`profile` only applies to nextest",
    "needs a `binary`",
    "duplicate custom check `x`",
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

  let project = Project::discover(&nested).unwrap();
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
  assert!(matches!(
    Project::discover(empty.path()),
    Err(ManifestError::NotFound { .. })
  ));
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
  let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../wkit.toml");
  let parsed = Manifest::load(&root).unwrap();
  assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
}
