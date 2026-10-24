use wkit_core::version::{Flavor, ProjectVersion};
use wkit_manifest::VersionFile;

use crate::{Change, Error, Mismatch, Result, Slot, locator::Locator};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionDocument {
  pub path: std::path::PathBuf,
  abs: std::path::PathBuf,
  text: String,
  slots: Vec<Slot>,
}

impl VersionDocument {
  pub fn load(root: &std::path::Path, file: &VersionFile) -> Result<Self> {
    Self::read(root, &file.path, Locator::of(file), file.flavor)
  }

  pub(crate) fn read(
    root: &std::path::Path,
    path: &std::path::Path,
    locator: Locator<'_>,
    flavor: Flavor,
  ) -> Result<Self> {
    let abs = root.join(path);
    let text = wkit_core::fs::read_to_string(&abs)?;
    Self::parse(path, abs, text, locator, flavor)
  }

  pub(crate) fn parse(
    path: &std::path::Path,
    abs: std::path::PathBuf,
    text: String,
    locator: Locator<'_>,
    flavor: Flavor,
  ) -> Result<Self> {
    let slots = locator.locate(&abs, &text, flavor)?;
    if slots.iter().all(|s| s.flavor().is_none()) {
      return Err(Error::NotFound {
        path: path.to_owned(),
      });
    }
    Ok(Self {
      path: path.to_owned(),
      abs,
      text,
      slots,
    })
  }

  pub fn current(&self) -> &str {
    self
      .versions()
      .next()
      .map(|(_, raw)| raw)
      .expect("documents hold at least one version")
  }

  pub fn versions(&self) -> impl Iterator<Item = (Flavor, &str)> {
    self
      .slots
      .iter()
      .filter_map(|s| s.flavor().map(|f| (f, &self.text[s.span.clone()])))
  }

  pub fn mismatches(&self, reference: &ProjectVersion) -> impl Iterator<Item = Mismatch> {
    self.versions().filter_map(move |(flavor, found)| {
      let expected = reference.render(flavor);
      (found != expected).then(|| Mismatch {
        path: self.path.clone(),
        found: found.to_owned(),
        expected,
      })
    })
  }

  pub fn change(&self, version: &ProjectVersion) -> Option<Change> {
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

#[cfg(test)]
mod tests {
  use wkit_core::version::Scheme;

  use super::*;

  fn doc(text: &str, locator: Locator<'_>, flavor: Flavor) -> Result<VersionDocument> {
    VersionDocument::parse(
      std::path::Path::new("f"),
      "/f".into(),
      text.to_owned(),
      locator,
      flavor,
    )
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
      change.contents(),
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
  }

  #[test]
  fn missing_versions_are_errors() {
    assert!(matches!(
      doc("pkgrel=1\n", Locator::Pkgbuild, Flavor::PkgVer),
      Err(Error::NotFound { .. })
    ));
  }
}
