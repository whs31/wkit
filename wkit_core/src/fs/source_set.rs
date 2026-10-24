use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct SourceSet {
  root: std::path::PathBuf,
  extensions: Vec<String>,
  include: Vec<String>,
  exclude: Vec<String>,
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
    }
  }

  #[must_use]
  pub fn extensions<I: IntoIterator<Item = S>, S: Into<String>>(mut self, extensions: I) -> Self {
    self.extensions = extensions
      .into_iter()
      .map(|e| e.into().trim_start_matches('.').to_ascii_lowercase())
      .collect();
    self
  }

  #[must_use]
  pub fn include<I: IntoIterator<Item = S>, S: Into<String>>(mut self, globs: I) -> Self {
    self.include = globs.into_iter().map(Into::into).collect();
    self
  }

  #[must_use]
  pub fn exclude<I: IntoIterator<Item = S>, S: Into<String>>(mut self, globs: I) -> Self {
    self.exclude = globs.into_iter().map(Into::into).collect();
    self
  }

  pub fn collect(&self) -> Result<Vec<std::path::PathBuf>> {
    let walk_error = |source| Error::Walk {
      root: self.root.clone(),
      source,
    };
    let mut overrides = ignore::overrides::OverrideBuilder::new(&self.root);
    for glob in &self.include {
      overrides.add(glob).map_err(walk_error)?;
    }
    for glob in &self.exclude {
      overrides.add(&format!("!{glob}")).map_err(walk_error)?;
    }
    let overrides = overrides.build().map_err(walk_error)?;

    let mut files = Vec::new();
    for entry in ignore::WalkBuilder::new(&self.root)
      .overrides(overrides)
      .require_git(false)
      .build()
    {
      let entry = entry.map_err(walk_error)?;
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
}
