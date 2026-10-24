use wkit_core::{Language, detect::BuildSystem, version::Scheme};

use super::Manifest;
use crate::{CheckSection, TestRunner, VersionFileKind, VersionSection};

pub(super) struct Validator<'a> {
  manifest: &'a Manifest,
  problems: Vec<String>,
}

impl<'a> Validator<'a> {
  pub(super) fn run(manifest: &'a Manifest) -> Vec<String> {
    let mut validator = Self {
      manifest,
      problems: Vec::new(),
    };
    validator.project();
    validator.bins();
    if let Some(version) = &manifest.version {
      validator.versions(version);
    }
    validator.toggles();
    validator.build();
    validator.tests();
    validator.tasks();
    validator.problems
  }

  fn report(&mut self, problem: impl Into<String>) {
    self.problems.push(problem.into());
  }

  fn project(&mut self) {
    let project = &self.manifest.project;
    if project.languages.is_empty() {
      self.report("`project.languages` must list at least one of \"c\", \"cpp\" or \"rust\"");
    }
    if project.name.as_deref().is_some_and(|n| n.trim().is_empty()) {
      self.report("`project.name` must not be empty");
    }
  }

  fn bins(&mut self) {
    let mut seen = std::collections::BTreeSet::new();
    for bin in &self.manifest.bins {
      if bin.name.trim().is_empty() {
        self.report("`[[bin]]` entries need a non-empty `name`");
      } else if !seen.insert(bin.name.as_str()) {
        self.report(format!("duplicate `[[bin]]` named `{}`", bin.name));
      }
    }
    if let Some(default) = &self.manifest.project.default_run
      && !seen.contains(default.as_str())
    {
      self.report(format!(
        "`project.default-run` refers to `{default}`, but no `[[bin]]` has that name"
      ));
    }
  }

  fn build(&mut self) {
    if self
      .manifest
      .conan
      .sync
      .remote
      .as_deref()
      .is_some_and(|r| r.trim().is_empty())
    {
      self.report("`conan.sync.remote` must not be empty");
    }
    if self
      .manifest
      .build
      .conan
      .settings
      .contains_key("build_type")
    {
      self.report(
        "`build.conan.settings` must not set `build_type`; it comes from the Conan profile or `--build-type`",
      );
    }
  }

  fn versions(&mut self, version: &VersionSection) {
    if version.files.is_empty() {
      self.report("`version.files` must list at least one file");
    }
    let mut seen = std::collections::BTreeSet::new();
    for file in &version.files {
      let path = file.path.display();
      if !seen.insert(&file.path) {
        self.report(format!("`{path}` is listed twice in `version.files`"));
      }
      if version.scheme == Scheme::CalVer && file.kind == VersionFileKind::Cargo {
        self.report(format!(
          "`{path}` cannot hold a calendar version; cargo requires semver"
        ));
      }
    }
    if let Some(source) = &version.source
      && !seen.contains(source)
    {
      self.report(format!(
        "`version.source` (`{}`) must be one of `version.files`",
        source.display()
      ));
    }
  }

  fn toggles(&mut self) {
    let m = self.manifest;
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
        self.report(format!(
          "{what} is enabled, but `project.languages` has no {lang}"
        ));
      }
    }
    if m.build.system == Some(BuildSystem::Cargo) && !rust {
      self.report("`build.system = \"cargo\"` requires Rust in `project.languages`");
    }
  }

  fn tests(&mut self) {
    let mut names = std::collections::BTreeSet::new();
    for suite in &self.manifest.test.suites {
      let name = suite.display_name();
      let runner = suite.runner;
      if !names.insert(name.clone()) {
        self.report(format!(
          "duplicate test suite `{name}`; give it a distinct `name`"
        ));
      }
      if !self
        .manifest
        .project
        .languages
        .iter()
        .any(|&l| runner.serves(l))
      {
        self.report(format!(
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
          self.report(format!(
            "test suite `{name}`: `{field}` only applies to {}",
            allowed.join(" and ")
          ));
        }
      }
      if runner == TestRunner::Catch2 && suite.binary.is_none() {
        self.report(format!(
          "test suite `{name}` runs catch2 and needs a `binary`"
        ));
      }
    }
  }

  fn tasks(&mut self) {
    let mut seen = std::collections::BTreeSet::new();
    for check in &self.manifest.check.custom {
      if check.name.trim().is_empty() || check.name.contains([' ', '\t', ',']) {
        self.report(format!(
          "custom check name `{}` must be non-empty and contain no spaces or commas",
          check.name
        ));
      } else if CheckSection::BUILTIN_STAGES.contains(&check.name.as_str()) {
        self.report(format!(
          "custom check `{}` clashes with the built-in check of that name",
          check.name
        ));
      } else if !seen.insert(check.name.as_str()) {
        self.report(format!("duplicate custom check `{}`", check.name));
      }
    }
    for name in self.manifest.scripts.keys() {
      if name.is_empty() || name.contains(char::is_whitespace) {
        self.report(format!(
          "script name `{name}` must be non-empty and contain no whitespace"
        ));
      }
    }
  }
}
