use wkit_core::version::{ProjectVersion, Scheme};
use wkit_manifest::{VersionFileKind, VersionSection};

use crate::{Change, Error, Mismatch, Result, VersionDocument, locator::Locator};

#[derive(Debug, Clone)]
pub struct VersionSet {
  scheme: Scheme,
  source: usize,
  files: Vec<VersionDocument>,
  companions: Vec<VersionDocument>,
}

impl VersionSet {
  pub const CARGO_LOCK: &str = "Cargo.lock";

  pub fn load(root: &std::path::Path, section: &VersionSection) -> Result<Self> {
    let mut files = Vec::with_capacity(section.files.len());
    let mut companions = Vec::new();
    for file in &section.files {
      let doc = VersionDocument::load(root, file)?;
      if file.kind == VersionFileKind::Cargo {
        let lock = file.path.with_file_name(Self::CARGO_LOCK);
        if let Some(text) = wkit_core::fs::read_optional(&root.join(&lock))? {
          let locator = Locator::CargoLock {
            version: doc.current(),
          };
          match VersionDocument::parse(&lock, root.join(&lock), text, locator, file.flavor) {
            Ok(lock) => companions.push(lock),
            Err(Error::NotFound { .. }) => {}
            Err(e) => return Err(e),
          }
        }
      }
      files.push(doc);
    }
    let source = section
      .source_file()
      .and_then(|s| section.files.iter().position(|f| f.path == s.path))
      .unwrap_or_default();
    Ok(Self {
      scheme: section.scheme,
      source,
      files,
      companions,
    })
  }

  pub fn scheme(&self) -> Scheme {
    self.scheme
  }

  pub fn files(&self) -> &[VersionDocument] {
    &self.files
  }

  pub fn source(&self) -> &VersionDocument {
    &self.files[self.source]
  }

  pub fn current(&self) -> Result<ProjectVersion> {
    let source = self.source();
    ProjectVersion::parse(source.current(), self.scheme).map_err(|e| Error::Invalid {
      path: source.path.clone(),
      source: e,
    })
  }

  pub fn mismatches(&self, reference: &ProjectVersion) -> Vec<Mismatch> {
    self
      .files
      .iter()
      .flat_map(|f| f.mismatches(reference))
      .collect()
  }

  pub fn changes(&self, version: &ProjectVersion) -> Vec<Change> {
    self
      .files
      .iter()
      .chain(&self.companions)
      .filter_map(|f| f.change(version))
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use wkit_core::version::Bump;
  use wkit_manifest::Manifest;

  use super::*;

  fn section(toml: &str) -> VersionSection {
    let manifest = format!("[project]\nlanguages = [\"cpp\", \"rust\"]\n\n[version]\n{toml}");
    Manifest::parse(&manifest)
      .unwrap()
      .manifest
      .version
      .unwrap()
  }

  fn write(root: &std::path::Path, path: &str, text: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }

  #[test]
  fn bumps_every_file_and_the_lock() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
      root,
      "conanfile.py",
      "class P(ConanFile):\n    version = \"1.2.3\"\n",
    );
    write(
      root,
      "CMakeLists.txt",
      "project(p\n  VERSION 1.2.3\n  LANGUAGES CXX)\n",
    );
    write(
      root,
      "Cargo.toml",
      "[package]\nname = \"p\"\nversion = \"1.2.3\"\n",
    );
    write(
      root,
      "Cargo.lock",
      "[[package]]\nname = \"p\"\nversion = \"1.2.3\"\n",
    );
    write(root, "pkg/PKGBUILD", "pkgver=1.2.3\npkgrel=2\n");
    write(root, "meta.lua", "return {\n  version = '1.2.3',\n}\n");
    let set = VersionSet::load(
      root,
      &section(
        "files = [\"conanfile.py\", \"CMakeLists.txt\", \"Cargo.toml\", \"pkg/PKGBUILD\", \"meta.lua\"]",
      ),
    )
    .unwrap();
    let current = set.current().unwrap();
    assert!(set.mismatches(&current).is_empty());

    let next = current.bumped_pre(Bump::Minor, "rc").unwrap();
    let changes = set.changes(&next);
    let paths: Vec<_> = changes.iter().map(|c| c.path.to_str().unwrap()).collect();
    assert_eq!(
      paths,
      [
        "conanfile.py",
        "CMakeLists.txt",
        "Cargo.toml",
        "pkg/PKGBUILD",
        "meta.lua",
        "Cargo.lock"
      ]
    );
    for change in &changes {
      change.apply().unwrap();
    }
    let read = |p: &str| std::fs::read_to_string(root.join(p)).unwrap();
    assert_eq!(
      read("CMakeLists.txt"),
      "project(p\n  VERSION 1.3.0\n  LANGUAGES CXX)\n"
    );
    assert_eq!(read("pkg/PKGBUILD"), "pkgver=1.3.0rc.1\npkgrel=1\n");
    assert_eq!(
      read("Cargo.lock"),
      "[[package]]\nname = \"p\"\nversion = \"1.3.0-rc.1\"\n"
    );
    assert_eq!(read("meta.lua"), "return {\n  version = '1.3.0-rc.1',\n}\n");
  }

  #[test]
  fn reports_mismatches_against_the_source() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
      root,
      "conanfile.py",
      "class P(ConanFile):\n    version = \"2.0.0\"\n",
    );
    write(root, "CMakeLists.txt", "project(p VERSION 1.9.0)\n");
    let set = VersionSet::load(
      root,
      &section("source = \"conanfile.py\"\nfiles = [\"CMakeLists.txt\", \"conanfile.py\"]"),
    )
    .unwrap();
    assert_eq!(set.source().path, std::path::Path::new("conanfile.py"));
    let current = set.current().unwrap();
    let mismatches = set.mismatches(&current);
    assert_eq!(mismatches.len(), 1);
    assert_eq!(mismatches[0].path, std::path::Path::new("CMakeLists.txt"));
  }

  #[test]
  fn missing_files_and_versions_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let missing = VersionSet::load(root, &section("files = [\"conanfile.py\"]"));
    assert!(matches!(
      missing,
      Err(Error::Core(wkit_core::Error::Read { .. }))
    ));
    write(
      root,
      "conanfile.py",
      "class P(ConanFile):\n    name = \"p\"\n",
    );
    let empty = VersionSet::load(root, &section("files = [\"conanfile.py\"]"));
    assert!(matches!(empty, Err(Error::NotFound { .. })));
  }
}
