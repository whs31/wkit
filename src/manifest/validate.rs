use std::collections::BTreeSet;

use super::{CheckSection, FileMapping, Manifest, TestRunner, VersionFileKind};
use crate::{Language, detect::BuildSystem, version::Scheme};

pub fn problems(m: &Manifest) -> Vec<String> {
  let mut problems = Vec::new();
  let mut report = |problem: String| problems.push(problem);
  project(m, &mut report);
  versions(m, &mut report);
  toggles(m, &mut report);
  build(m, &mut report);
  tests(m, &mut report);
  tasks(m, &mut report);
  packaging(m, &mut report);
  problems
}

type Report<'a> = dyn FnMut(String) + 'a;

fn project(m: &Manifest, report: &mut Report<'_>) {
  if m.project.languages.is_empty() {
    report("`project.languages` must list at least one of \"c\", \"cpp\" or \"rust\"".into());
  }
  if m
    .project
    .name
    .as_deref()
    .is_some_and(|n| n.trim().is_empty())
  {
    report("`project.name` must not be empty".into());
  }
  let mut seen = BTreeSet::new();
  for bin in &m.bins {
    if bin.name.trim().is_empty() {
      report("`[[bin]]` entries need a non-empty `name`".into());
    } else if !seen.insert(bin.name.as_str()) {
      report(format!("duplicate `[[bin]]` named `{}`", bin.name));
    }
  }
  if let Some(default) = &m.project.default_run
    && !seen.contains(default.as_str())
  {
    report(format!(
      "`project.default-run` refers to `{default}`, but no `[[bin]]` has that name"
    ));
  }
}

fn versions(m: &Manifest, report: &mut Report<'_>) {
  let Some(version) = &m.version else {
    return;
  };
  if version.files.is_empty() {
    report("`version.files` must list at least one file".into());
  }
  let mut seen = BTreeSet::new();
  for file in &version.files {
    let path = file.path.display();
    if !seen.insert(&file.path) {
      report(format!("`{path}` is listed twice in `version.files`"));
    }
    if version.scheme == Scheme::CalVer && file.kind == VersionFileKind::Cargo {
      report(format!(
        "`{path}` cannot hold a calendar version; cargo requires semver"
      ));
    }
  }
  if let Some(source) = &version.source
    && !seen.contains(source)
  {
    report(format!(
      "`version.source` (`{}`) must be one of `version.files`",
      source.display()
    ));
  }
}

fn toggles(m: &Manifest, report: &mut Report<'_>) {
  let rust = m.has(Language::Rust);
  let c = m.has_c_family();
  for (enabled, ok, what, lang) in [
    (m.format.rust.enabled, rust, "`format.rust`", "Rust"),
    (m.check.clippy.enabled, rust, "`check.clippy`", "Rust"),
    (m.format.cpp.enabled, c, "`format.cpp`", "C or C++"),
    (
      m.check.clang_tidy.enabled,
      c,
      "`check.clang-tidy`",
      "C or C++",
    ),
  ] {
    if enabled == Some(true) && !ok {
      report(format!(
        "{what} is enabled, but `project.languages` has no {lang}"
      ));
    }
  }
  if m.build.system == Some(BuildSystem::Cargo) && !rust {
    report("`build.system = \"cargo\"` requires Rust in `project.languages`".into());
  }
}

fn build(m: &Manifest, report: &mut Report<'_>) {
  if m
    .conan
    .sync
    .remote
    .as_deref()
    .is_some_and(|r| r.trim().is_empty())
  {
    report("`conan.sync.remote` must not be empty".into());
  }
  if m.build.conan.settings.contains_key("build_type") {
    report(
      "`build.conan.settings` must not set `build_type`; it comes from the Conan profile or `--build-type`".into(),
    );
  }
}

fn tests(m: &Manifest, report: &mut Report<'_>) {
  let mut names = BTreeSet::new();
  for suite in &m.test.suites {
    let name = suite.display_name();
    let runner = suite.runner;
    if !names.insert(name.clone()) {
      report(format!(
        "duplicate test suite `{name}`; give it a distinct `name`"
      ));
    }
    if !m.project.languages.iter().any(|&l| runner.serves(l)) {
      report(format!(
        "test suite `{name}` uses {runner}, which does not fit `project.languages`"
      ));
    }
    for (set, field, allowed) in [
      (
        suite.profile.is_some(),
        "profile",
        &[TestRunner::Nextest][..],
      ),
      (suite.binary.is_some(), "binary", &[TestRunner::Catch2]),
      (
        suite.build_dir.is_some(),
        "build-dir",
        &[TestRunner::Ctest, TestRunner::Catch2],
      ),
    ] {
      if set && !allowed.contains(&runner) {
        let allowed: Vec<_> = allowed.iter().map(ToString::to_string).collect();
        report(format!(
          "test suite `{name}`: `{field}` only applies to {}",
          allowed.join(" and ")
        ));
      }
    }
    if runner == TestRunner::Catch2 && suite.binary.is_none() {
      report(format!(
        "test suite `{name}` runs catch2 and needs a `binary`"
      ));
    }
  }
}

fn tasks(m: &Manifest, report: &mut Report<'_>) {
  let mut seen = BTreeSet::new();
  for check in &m.check.custom {
    let name = check.name.as_str();
    if name.trim().is_empty() || name.contains([' ', '\t', ',']) {
      report(format!(
        "custom check name `{name}` must be non-empty and contain no spaces or commas"
      ));
    } else if CheckSection::BUILTIN_STAGES.contains(&name) {
      report(format!(
        "custom check `{name}` clashes with the built-in check of that name"
      ));
    } else if !seen.insert(name) {
      report(format!("duplicate custom check `{name}`"));
    }
  }
  for name in m.scripts.keys() {
    if name.is_empty() || name.contains(char::is_whitespace) {
      report(format!(
        "script name `{name}` must be non-empty and contain no whitespace"
      ));
    }
  }
}

fn packaging(m: &Manifest, report: &mut Report<'_>) {
  for (value, field) in [
    (&m.package.name, "package.name"),
    (&m.package.set_option, "package.set-option"),
  ] {
    if value.as_deref().is_some_and(|v| v.trim().is_empty()) {
      report(format!("`{field}` must not be empty"));
    }
  }
  for (files, table) in [(&m.package.files, "package"), (&m.install.files, "install")] {
    for file in files {
      file_mapping(file, table, report);
    }
  }
  if m
    .install
    .prefix
    .as_ref()
    .is_some_and(|prefix| !prefix.has_root())
  {
    report("`install.prefix` must be an absolute path".into());
  }
}

fn file_mapping(file: &FileMapping, table: &str, report: &mut Report<'_>) {
  if !file.to.starts_with('/') {
    report(format!(
      "`[{table}] files`: `{}` must be an absolute path",
      file.to
    ));
  }
  if file.is_glob() && !file.targets_dir() {
    report(format!(
      "`[{table}] files`: `{}` matches several files, so `to` must end with `/`",
      file.from
    ));
  }
  if file.config && table != "install" {
    report(format!(
      "`[{table}] files`: `config` only applies to `[install] files`; packages mark files under /etc as configuration"
    ));
  }
}
