#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, derive_more::Display)]
#[serde(rename_all = "lowercase")]
#[display(rename_all = "lowercase")]
pub enum BuildSystem {
  Cargo,
  Conan,
  CMake,
}

impl BuildSystem {
  pub const PRECEDENCE: [Self; 3] = [Self::Conan, Self::Cargo, Self::CMake];

  pub fn marker(self) -> &'static str {
    match self {
      Self::Cargo => "Cargo.toml",
      Self::Conan => "conanfile.py",
      Self::CMake => "CMakeLists.txt",
    }
  }

  pub fn from_path(root: &std::path::Path) -> Option<Self> {
    Self::PRECEDENCE
      .into_iter()
      .find(|system| root.join(system.marker()).is_file())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn conan_wins_over_cargo_and_cmake() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    assert_eq!(BuildSystem::from_path(root), None);
    std::fs::write(root.join("CMakeLists.txt"), "").unwrap();
    assert_eq!(BuildSystem::from_path(root), Some(BuildSystem::CMake));
    std::fs::write(root.join("Cargo.toml"), "").unwrap();
    assert_eq!(BuildSystem::from_path(root), Some(BuildSystem::Cargo));
    std::fs::write(root.join("conanfile.py"), "").unwrap();
    assert_eq!(BuildSystem::from_path(root), Some(BuildSystem::Conan));
  }

  #[test]
  fn displays_like_manifest_values() {
    assert_eq!(BuildSystem::CMake.to_string(), "cmake");
  }
}
