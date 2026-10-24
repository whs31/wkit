use anyhow::Context as _;

use super::{Flavor, ProjectVersion, Scheme, locator::Locator};
use crate::{
  Result,
  manifest::{VersionFile, VersionFileKind, VersionSection},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotValue {
  Version(Flavor),
  Literal(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
  pub span: std::ops::Range<usize>,
  pub value: SlotValue,
}

impl Slot {
  pub fn version(span: std::ops::Range<usize>, flavor: Flavor) -> Self {
    Self {
      span,
      value: SlotValue::Version(flavor),
    }
  }

  fn flavor(&self) -> Option<Flavor> {
    match self.value {
      SlotValue::Version(flavor) => Some(flavor),
      SlotValue::Literal(_) => None,
    }
  }

  fn render(&self, version: &ProjectVersion) -> String {
    match self.value {
      SlotValue::Version(flavor) => version.render(flavor),
      SlotValue::Literal(literal) => literal.to_owned(),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
  pub path: std::path::PathBuf,
  pub from: String,
  pub to: String,
  abs: std::path::PathBuf,
  contents: String,
}

impl Change {
  pub fn apply(&self) -> Result<()> {
    Ok(fs_err::write(&self.abs, &self.contents)?)
  }
}

#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
#[display("`{}` has {found}, expected {expected}", path.display())]
pub struct Mismatch {
  pub path: std::path::PathBuf,
  pub found: String,
  pub expected: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionDocument {
  pub path: std::path::PathBuf,
  abs: std::path::PathBuf,
  text: String,
  slots: Vec<Slot>,
}

impl VersionDocument {
  pub fn load(root: &std::path::Path, file: &VersionFile) -> Result<Self> {
    let abs = root.join(&file.path);
    let text = fs_err::read_to_string(&abs)?;
    Self::parse(&file.path, abs, text, Locator::of(file), file.flavor)?
      .with_context(|| format!("could not find a version in `{}`", file.path.display()))
  }

  fn parse(
    path: &std::path::Path,
    abs: std::path::PathBuf,
    text: String,
    locator: Locator<'_>,
    flavor: Flavor,
  ) -> Result<Option<Self>> {
    let slots = locator.locate(&abs, &text, flavor)?;
    Ok(slots.iter().any(|s| s.flavor().is_some()).then(|| Self {
      path: path.to_owned(),
      abs,
      text,
      slots,
    }))
  }

  pub fn current(&self) -> &str {
    self
      .versions()
      .next()
      .map(|(_, raw)| raw)
      .expect("documents hold at least one version")
  }

  fn versions(&self) -> impl Iterator<Item = (Flavor, &str)> {
    self
      .slots
      .iter()
      .filter_map(|s| s.flavor().map(|f| (f, &self.text[s.span.clone()])))
  }

  fn mismatches(&self, reference: &ProjectVersion) -> impl Iterator<Item = Mismatch> {
    self.versions().filter_map(move |(flavor, found)| {
      let expected = reference.render(flavor);
      (found != expected).then(|| Mismatch {
        path: self.path.clone(),
        found: found.to_owned(),
        expected,
      })
    })
  }

  fn change(&self, version: &ProjectVersion) -> Option<Change> {
    let mut slots: Vec<&Slot> = self.slots.iter().collect();
    slots.sort_by_key(|s| std::cmp::Reverse(s.span.start));
    let mut contents = self.text.clone();
    for slot in &slots {
      contents.replace_range(slot.span.clone(), &slot.render(version));
    }
    let to = self
      .slots
      .iter()
      .find_map(|s| s.flavor().map(|f| version.render(f)))
      .expect("documents hold at least one version");
    (contents != self.text).then(|| Change {
      path: self.path.clone(),
      from: self.current().to_owned(),
      to,
      abs: self.abs.clone(),
      contents,
    })
  }
}

#[derive(Debug, Clone)]
pub struct VersionSet {
  scheme: Scheme,
  source: usize,
  files: Vec<VersionDocument>,
  companions: Vec<VersionDocument>,
}

impl VersionSet {
  pub fn load(root: &std::path::Path, section: &VersionSection) -> Result<Self> {
    let mut files = Vec::with_capacity(section.files.len());
    let mut companions = Vec::new();
    for file in &section.files {
      let doc = VersionDocument::load(root, file)?;
      if file.kind == VersionFileKind::Cargo {
        let lock = file.path.with_file_name("Cargo.lock");
        if let Some(text) = crate::fs::read_optional(&root.join(&lock))? {
          let locator = Locator::CargoLock {
            version: doc.current(),
          };
          companions.extend(VersionDocument::parse(
            &lock,
            root.join(&lock),
            text,
            locator,
            file.flavor,
          )?);
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

  pub fn current(&self) -> Result<ProjectVersion> {
    let source = &self.files[self.source];
    ProjectVersion::parse(source.current(), self.scheme)
      .with_context(|| format!("`{}` has an unusable version", source.path.display()))
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
  use super::*;
  use crate::{manifest::Manifest, version::Bump};

  fn doc(text: &str, locator: Locator<'_>, flavor: Flavor) -> Option<VersionDocument> {
    VersionDocument::parse(
      std::path::Path::new("f"),
      "/f".into(),
      text.to_owned(),
      locator,
      flavor,
    )
    .unwrap()
  }

  fn sem(v: &str) -> ProjectVersion {
    ProjectVersion::parse(v, Scheme::SemVer).unwrap()
  }

  #[test]
  fn rewrites_every_slot_and_keeps_the_rest() {
    let text = "# keep\n_source_version=1.2.0\npkgver=1.2.0 # keep\npkgrel=4\n";
    let d = doc(text, Locator::Pkgbuild, Flavor::PkgVer).unwrap();
    assert_eq!(d.current(), "1.2.0");
    let change = d.change(&sem("1.3.0-rc.1")).unwrap();
    assert_eq!(
      change.contents,
      "# keep\n_source_version=1.3.0-rc.1\npkgver=1.3.0rc.1 # keep\npkgrel=1\n"
    );
    assert_eq!(
      (change.from.as_str(), change.to.as_str()),
      ("1.2.0", "1.3.0-rc.1")
    );
  }

  #[test]
  fn unchanged_documents_produce_no_change() {
    let d = doc("project(x VERSION 1.2.0)", Locator::CMake, Flavor::Numeric).unwrap();
    assert_eq!(d.change(&sem("1.2.0-rc.1")), None);
    assert!(d.mismatches(&sem("1.2.0-rc.1")).next().is_none());
    let mismatch = d.mismatches(&sem("1.2.1")).next().unwrap();
    assert_eq!(mismatch.to_string(), "`f` has 1.2.0, expected 1.2.1");
    assert_eq!(doc("pkgrel=1\n", Locator::Pkgbuild, Flavor::PkgVer), None);
  }

  fn section(toml: &str) -> VersionSection {
    let manifest = format!("[project]\nlanguages = [\"cpp\", \"rust\"]\n\n[version]\n{toml}");
    Manifest::parse(&manifest).unwrap().version.unwrap()
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
      &section("files = [\"conanfile.py\", \"CMakeLists.txt\", \"Cargo.toml\", \"pkg/PKGBUILD\", \"meta.lua\"]"),
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
    let current = set.current().unwrap();
    assert_eq!(current.to_string(), "2.0.0");
    let mismatches = set.mismatches(&current);
    assert_eq!(mismatches.len(), 1);
    assert_eq!(mismatches[0].path, std::path::Path::new("CMakeLists.txt"));
  }

  #[test]
  fn missing_files_and_versions_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    assert!(VersionSet::load(root, &section("files = [\"conanfile.py\"]")).is_err());
    write(
      root,
      "conanfile.py",
      "class P(ConanFile):\n    name = \"p\"\n",
    );
    let err = VersionSet::load(root, &section("files = [\"conanfile.py\"]")).unwrap_err();
    assert_eq!(
      err.to_string(),
      "could not find a version in `conanfile.py`"
    );
  }
}
