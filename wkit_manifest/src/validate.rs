use std::collections::BTreeSet;

use wkit_core::{Language, detect::BuildSystem, version::Scheme};

use crate::{
  Manifest,
  test::TestRunner,
  version::{VersionFileKind, VersionSection},
};

pub(crate) fn validate(m: &Manifest) -> Vec<String> {
  let mut problems = Vec::new();
  project(m, &mut problems);
  bins(m, &mut problems);
  if let Some(version) = &m.version {
    versions(version, &mut problems);
  }
  toggles(m, &mut problems);
  tests(m, &mut problems);
  tasks(m, &mut problems);
  problems
}

fn project(m: &Manifest, problems: &mut Vec<String>) {
  if m.project.languages.is_empty() {
    problems
      .push("`project.languages` must list at least one of \"c\", \"cpp\" or \"rust\"".into());
  }
  if m
    .project
    .name
    .as_deref()
    .is_some_and(|n| n.trim().is_empty())
  {
    problems.push("`project.name` must not be empty".into());
  }
}

fn bins(m: &Manifest, problems: &mut Vec<String>) {
  let mut seen = BTreeSet::new();
  for bin in &m.bins {
    if bin.name.trim().is_empty() {
      problems.push("`[[bin]]` entries need a non-empty `name`".into());
    } else if !seen.insert(bin.name.as_str()) {
      problems.push(format!("duplicate `[[bin]]` named `{}`", bin.name));
    }
  }
  if let Some(default) = &m.project.default_run
    && !seen.contains(default.as_str())
  {
    problems.push(format!(
      "`project.default-run` refers to `{default}`, but no `[[bin]]` has that name"
    ));
  }
}

fn versions(v: &VersionSection, problems: &mut Vec<String>) {
  if v.files.is_empty() {
    problems.push("`version.files` must list at least one file".into());
  }
  let mut seen = BTreeSet::new();
  for file in &v.files {
    if !seen.insert(&file.path) {
      problems.push(format!(
        "`{}` is listed twice in `version.files`",
        file.path.display()
      ));
    }
    if v.scheme == Scheme::CalVer && file.kind == VersionFileKind::Cargo {
      problems.push(format!(
        "`{}` cannot hold a calendar version; cargo requires semver",
        file.path.display()
      ));
    }
  }
  if let Some(source) = &v.source
    && !seen.contains(source)
  {
    problems.push(format!(
      "`version.source` (`{}`) must be one of `version.files`",
      source.display()
    ));
  }
}

fn toggles(m: &Manifest, problems: &mut Vec<String>) {
  let rust = m.has(Language::Rust);
  let c = m.has_c_family();
  let mut require = |enabled: Option<bool>, ok: bool, what: &str, lang: &str| {
    if enabled == Some(true) && !ok {
      problems.push(format!(
        "{what} is enabled, but `project.languages` has no {lang}"
      ));
    }
  };
  require(m.format.rust.enabled, rust, "`format.rust`", "Rust");
  require(m.check.clippy.enabled, rust, "`check.clippy`", "Rust");
  require(m.format.cpp.enabled, c, "`format.cpp`", "C or C++");
  require(
    m.check.clang_tidy.enabled,
    c,
    "`check.clang-tidy`",
    "C or C++",
  );
  if m.build.system == Some(BuildSystem::Cargo) && !rust {
    problems.push("`build.system = \"cargo\"` requires Rust in `project.languages`".into());
  }
}

fn tests(m: &Manifest, problems: &mut Vec<String>) {
  let mut names = BTreeSet::new();
  for suite in &m.test.suites {
    let name = suite.display_name();
    let runner = suite.runner;
    if !names.insert(name.clone()) {
      problems.push(format!(
        "duplicate test suite `{name}`; give it a distinct `name`"
      ));
    }
    if !m.project.languages.iter().any(|&l| runner.serves(l)) {
      problems.push(format!(
        "test suite `{name}` uses {runner}, which does not fit `project.languages`"
      ));
    }
    let mut only = |set: bool, field: &str, allowed: &[TestRunner]| {
      if set && !allowed.contains(&runner) {
        let allowed: Vec<_> = allowed.iter().map(ToString::to_string).collect();
        problems.push(format!(
          "test suite `{name}`: `{field}` only applies to {}",
          allowed.join(" and ")
        ));
      }
    };
    only(suite.profile.is_some(), "profile", &[TestRunner::Nextest]);
    only(suite.binary.is_some(), "binary", &[TestRunner::Catch2]);
    only(
      suite.build_dir.is_some(),
      "build-dir",
      &[TestRunner::Ctest, TestRunner::Catch2],
    );
    if runner == TestRunner::Catch2 && suite.binary.is_none() {
      problems.push(format!(
        "test suite `{name}` runs catch2 and needs a `binary`"
      ));
    }
  }
}

fn tasks(m: &Manifest, problems: &mut Vec<String>) {
  let mut seen = BTreeSet::new();
  for check in &m.check.custom {
    if check.name.trim().is_empty() {
      problems.push("`[[check.custom]]` entries need a non-empty `name`".into());
    } else if !seen.insert(check.name.as_str()) {
      problems.push(format!("duplicate custom check `{}`", check.name));
    }
  }
  for name in m.scripts.keys() {
    if name.is_empty() || name.contains(char::is_whitespace) {
      problems.push(format!(
        "script name `{name}` must be non-empty and contain no whitespace"
      ));
    }
  }
}
