use anyhow::Context as _;

use crate::Result;

pub fn find_upwards(start: &std::path::Path, file_name: &str) -> Option<std::path::PathBuf> {
  start
    .ancestors()
    .map(|dir| dir.join(file_name))
    .find(|candidate| candidate.is_file())
}

pub fn read_optional(path: &std::path::Path) -> Result<Option<String>> {
  match fs_err::read_to_string(path) {
    Ok(text) => Ok(Some(text)),
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
    Err(e) => Err(e.into()),
  }
}

pub fn find_executable(dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
  let exe = std::path::PathBuf::from(format!("{name}{}", std::env::consts::EXE_SUFFIX));
  let direct = dir.join(&exe);
  if direct.is_file() {
    return Some(direct);
  }
  let file_name = exe.file_name()?;
  ignore::WalkBuilder::new(dir)
    .standard_filters(false)
    .filter_entry(|entry| entry.file_name() != "CMakeFiles")
    .build()
    .flatten()
    .filter(|entry| {
      entry.file_type().is_some_and(|t| t.is_file()) && entry.file_name() == file_name
    })
    .map(ignore::DirEntry::into_path)
    .min()
}

#[derive(Debug, Clone)]
pub struct SourceSet {
  root: std::path::PathBuf,
  extensions: Vec<String>,
  include: Vec<String>,
  exclude: Vec<String>,
  hidden: bool,
}

impl SourceSet {
  pub const DEFAULT_EXCLUDES: &[&str] = &[
    "/build/",
    "/target/",
    "/vendor/",
    "/packaging/",
    "/cmake-build-*/",
    "/cmake-visual-studio-*/",
    "node_modules/",
    ".venv/",
  ];

  pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
    Self {
      root: root.into(),
      extensions: Vec::new(),
      include: Vec::new(),
      exclude: Self::DEFAULT_EXCLUDES
        .iter()
        .map(|&s| s.to_owned())
        .collect(),
      hidden: false,
    }
  }

  #[must_use]
  pub fn hidden(mut self, hidden: bool) -> Self {
    self.hidden = hidden;
    self
  }

  #[must_use]
  pub fn extensions(mut self, extensions: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
    self.extensions = extensions
      .into_iter()
      .map(|e| e.as_ref().trim_start_matches('.').to_ascii_lowercase())
      .collect();
    self
  }

  #[must_use]
  pub fn include(mut self, globs: impl IntoIterator<Item = impl Into<String>>) -> Self {
    self.include = globs.into_iter().map(Into::into).collect();
    self
  }

  #[must_use]
  pub fn exclude(mut self, globs: impl IntoIterator<Item = impl Into<String>>) -> Self {
    self.exclude = globs.into_iter().map(Into::into).collect();
    self
  }

  pub fn collect(&self) -> Result<Vec<std::path::PathBuf>> {
    self
      .walk()
      .with_context(|| format!("failed to walk `{}`", self.root.display()))
  }

  fn walk(&self) -> Result<Vec<std::path::PathBuf>, ignore::Error> {
    let mut overrides = ignore::overrides::OverrideBuilder::new(&self.root);
    for glob in &self.include {
      overrides.add(glob)?;
    }
    for glob in &self.exclude {
      overrides.add(&format!("!{glob}"))?;
    }
    if self.hidden {
      overrides.add("!.git/")?;
    }
    let mut files = Vec::new();
    for entry in ignore::WalkBuilder::new(&self.root)
      .overrides(overrides.build()?)
      .hidden(!self.hidden)
      .require_git(false)
      .build()
    {
      let entry = entry?;
      if entry.file_type().is_some_and(|t| t.is_file()) && self.matches_extension(entry.path()) {
        files.push(entry.into_path());
      }
    }
    files.sort();
    Ok(files)
  }

  fn matches_extension(&self, path: &std::path::Path) -> bool {
    self.extensions.is_empty()
      || path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| self.extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
  }
}

#[cfg(test)]
mod tests {
  use std::path::{Path, PathBuf};

  use super::*;

  fn touch(root: &Path, rel: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "").unwrap();
  }

  fn rel(root: &Path, files: Vec<PathBuf>) -> Vec<String> {
    files
      .into_iter()
      .map(|f| {
        f.strip_prefix(root)
          .unwrap()
          .to_string_lossy()
          .replace('\\', "/")
      })
      .collect()
  }

  #[test]
  fn finds_file_in_ancestors() {
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), "wkit.toml");
    let nested = dir.path().join("a/b");
    std::fs::create_dir_all(&nested).unwrap();
    assert_eq!(
      find_upwards(&nested, "wkit.toml"),
      Some(dir.path().join("wkit.toml"))
    );
    assert_eq!(find_upwards(&nested, "nope.toml"), None);
  }

  #[test]
  fn finds_executables_outside_cmake_internals() {
    let dir = tempfile::tempdir().unwrap();
    let exe = |rel: &str| {
      dir
        .path()
        .join(format!("{rel}{}", std::env::consts::EXE_SUFFIX))
    };
    for rel in ["CMakeFiles/app", "tools/app", "z/app"] {
      touch(
        dir.path(),
        &format!("{rel}{}", std::env::consts::EXE_SUFFIX),
      );
    }
    assert_eq!(find_executable(dir.path(), "app"), Some(exe("tools/app")));
    std::fs::write(exe("app"), "").unwrap();
    assert_eq!(find_executable(dir.path(), "app"), Some(exe("app")));
    assert_eq!(find_executable(dir.path(), "nope"), None);
  }

  #[test]
  fn optional_read_distinguishes_missing_from_failing() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(read_optional(&dir.path().join("missing")).unwrap(), None);
    assert!(read_optional(dir.path()).is_err());
  }

  #[test]
  fn collects_sources_skipping_build_trees() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for f in [
      "src/a.cc",
      "src/a.h",
      "src/b.CPP",
      "src/build/gen.cc",
      "build/Debug/x.cc",
      "vendor/v.cc",
      "cmake-build-debug/y.cc",
      "README.md",
    ] {
      touch(root, f);
    }
    let files = SourceSet::new(root)
      .extensions(["cc", ".h", "cpp"])
      .collect()
      .unwrap();
    assert_eq!(
      rel(root, files),
      ["src/a.cc", "src/a.h", "src/b.CPP", "src/build/gen.cc"]
    );
  }

  #[test]
  fn include_restricts_and_gitignore_is_honored() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for f in ["src/a.cc", "tests/t.cc", "gen/g.cc"] {
      touch(root, f);
    }
    std::fs::write(root.join(".gitignore"), "gen/\n").unwrap();
    let all = SourceSet::new(root).extensions(["cc"]).collect().unwrap();
    assert_eq!(rel(root, all), ["src/a.cc", "tests/t.cc"]);
    let only_src = SourceSet::new(root)
      .extensions(["cc"])
      .include(["src/**"])
      .collect()
      .unwrap();
    assert_eq!(rel(root, only_src), ["src/a.cc"]);
  }

  #[test]
  fn hidden_files_are_opt_in() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for f in [".env", ".config/x.toml", ".git/config", "a.txt"] {
      touch(root, f);
    }
    assert_eq!(
      rel(root, SourceSet::new(root).collect().unwrap()),
      ["a.txt"]
    );
    assert_eq!(
      rel(root, SourceSet::new(root).hidden(true).collect().unwrap()),
      [".config/x.toml", ".env", "a.txt"]
    );
  }
}
