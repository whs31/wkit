use std::{
  io,
  path::{Path, PathBuf},
};

use ignore::{WalkBuilder, overrides::OverrideBuilder};

use crate::{Error, Result};

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

#[derive(Debug, Clone)]
pub struct SourceSet {
  root: PathBuf,
  extensions: Vec<String>,
  include: Vec<String>,
  exclude: Vec<String>,
}

impl SourceSet {
  pub fn new(root: impl Into<PathBuf>) -> Self {
    Self {
      root: root.into(),
      extensions: Vec::new(),
      include: Vec::new(),
      exclude: DEFAULT_EXCLUDES.iter().map(|&s| s.to_owned()).collect(),
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

  pub fn collect(&self) -> Result<Vec<PathBuf>> {
    let walk_error = |source| Error::Walk {
      root: self.root.clone(),
      source,
    };
    let mut overrides = OverrideBuilder::new(&self.root);
    for glob in &self.include {
      overrides.add(glob).map_err(walk_error)?;
    }
    for glob in &self.exclude {
      overrides.add(&format!("!{glob}")).map_err(walk_error)?;
    }
    let overrides = overrides.build().map_err(walk_error)?;

    let mut files = Vec::new();
    for entry in WalkBuilder::new(&self.root)
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

  fn matches_extension(&self, path: &Path) -> bool {
    self.extensions.is_empty()
      || path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| self.extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
  }
}

pub fn find_upwards(start: &Path, file_name: &str) -> Option<PathBuf> {
  start
    .ancestors()
    .map(|dir| dir.join(file_name))
    .find(|candidate| candidate.is_file())
}

pub fn read_to_string(path: &Path) -> Result<String> {
  std::fs::read_to_string(path).map_err(|source| Error::Read {
    path: path.to_owned(),
    source,
  })
}

pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> Result<()> {
  std::fs::write(path, contents).map_err(|source| Error::Write {
    path: path.to_owned(),
    source,
  })
}

pub fn read_optional(path: &Path) -> Result<Option<String>> {
  match std::fs::read_to_string(path) {
    Ok(text) => Ok(Some(text)),
    Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
    Err(source) => Err(Error::Read {
      path: path.to_owned(),
      source,
    }),
  }
}

#[cfg(test)]
mod tests {
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

  #[test]
  fn finds_file_in_ancestors() {
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), "wkit.toml");
    std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
    assert_eq!(
      find_upwards(&dir.path().join("a/b"), "wkit.toml"),
      Some(dir.path().join("wkit.toml"))
    );
    assert_eq!(find_upwards(&dir.path().join("a/b"), "nope.toml"), None);
  }
}
