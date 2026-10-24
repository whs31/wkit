#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ArchPackage {
  pub dir: Option<std::path::PathBuf>,
}

impl ArchPackage {
  pub const PKGBUILD: &str = "PKGBUILD";
  pub const DEFAULT_DIRS: &[&str] = &["packaging/arch", "pkg/arch"];

  pub fn locate(&self, root: &std::path::Path) -> Option<std::path::PathBuf> {
    match &self.dir {
      Some(dir) => Some(root.join(dir)),
      None => Self::DEFAULT_DIRS
        .iter()
        .map(|dir| root.join(dir))
        .find(|dir| dir.join(Self::PKGBUILD).is_file()),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn finds_the_pkgbuild_in_known_dirs() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(ArchPackage::default().locate(dir.path()), None);
    std::fs::create_dir_all(dir.path().join("pkg/arch")).unwrap();
    std::fs::write(dir.path().join("pkg/arch/PKGBUILD"), "").unwrap();
    assert_eq!(
      ArchPackage::default().locate(dir.path()),
      Some(dir.path().join("pkg/arch"))
    );
    let explicit = ArchPackage {
      dir: Some("dist".into()),
    };
    assert_eq!(explicit.locate(dir.path()), Some(dir.path().join("dist")));
  }
}
