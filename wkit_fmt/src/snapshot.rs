use wkit_core::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot(Vec<u64>);

impl Snapshot {
  pub fn take(root: &std::path::Path, sources: &[std::path::PathBuf]) -> Result<Self> {
    sources
      .iter()
      .map(|path| {
        let path = root.join(path);
        let bytes = std::fs::read(&path).map_err(|source| Error::Read { path, source })?;
        Ok(std::hash::BuildHasher::hash_one(
          &std::hash::BuildHasherDefault::<std::hash::DefaultHasher>::default(),
          bytes,
        ))
      })
      .collect::<Result<_>>()
      .map(Self)
  }

  pub fn changed(
    &self,
    root: &std::path::Path,
    sources: &[std::path::PathBuf],
  ) -> Result<Vec<std::path::PathBuf>> {
    let after = Self::take(root, sources)?;
    Ok(
      sources
        .iter()
        .zip(self.0.iter().zip(after.0))
        .filter(|(_, (before, after))| **before != *after)
        .map(|(path, _)| path.clone())
        .collect(),
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn only_rewritten_files_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let sources = vec![std::path::PathBuf::from("a"), std::path::PathBuf::from("b")];
    for name in &sources {
      std::fs::write(dir.path().join(name), "x").unwrap();
    }
    let before = Snapshot::take(dir.path(), &sources).unwrap();
    std::fs::write(dir.path().join("b"), "y").unwrap();
    assert_eq!(
      before.changed(dir.path(), &sources).unwrap(),
      [std::path::PathBuf::from("b")]
    );
  }
}
