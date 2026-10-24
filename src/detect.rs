use anyhow::Context as _;

use crate::{Result, fs::SourceSet};

#[derive(
  Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Deserialize, derive_more::Display,
)]
#[serde(rename_all = "lowercase")]
pub enum Language {
  C,

  #[serde(alias = "c++", alias = "cxx")]
  #[display("C++")]
  Cpp,

  #[serde(alias = "rs")]
  Rust,
}

impl Language {
  pub fn is_c_family(self) -> bool {
    matches!(self, Self::C | Self::Cpp)
  }

  pub fn from_extension(ext: &str) -> Option<Self> {
    match ext.to_ascii_lowercase().as_str() {
      "c" => Some(Self::C),
      "cc" | "cpp" | "cxx" | "c++" | "hh" | "hpp" | "hxx" | "ipp" | "cu" | "cuh" => Some(Self::Cpp),
      "rs" => Some(Self::Rust),
      _ => None,
    }
  }

  pub fn detect(root: &std::path::Path) -> Result<std::collections::BTreeSet<Self>> {
    let mut found = std::collections::BTreeSet::new();
    if root.join("Cargo.toml").is_file() {
      found.insert(Self::Rust);
    }
    for file in SourceSet::new(root).collect()? {
      if let Some(lang) = file
        .extension()
        .and_then(|e| e.to_str())
        .and_then(Self::from_extension)
      {
        found.insert(lang);
      }
    }
    Ok(found)
  }
}

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

const RUSTFMT_CONFIGS: &[&str] = &[".rustfmt.toml", "rustfmt.toml"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, derive_more::Display)]
#[serde(rename_all = "lowercase")]
#[display(rename_all = "lowercase")]
pub enum RustfmtChannel {
  Stable,
  Nightly,
}

impl RustfmtChannel {
  pub fn from_path(root: &std::path::Path) -> Result<Self> {
    let config = root
      .ancestors()
      .flat_map(|dir| RUSTFMT_CONFIGS.iter().map(move |name| dir.join(name)))
      .find(|path| path.is_file());
    let Some(path) = config else {
      return Ok(Self::Stable);
    };
    let table: toml::Table = toml::from_str(&fs_err::read_to_string(&path)?)
      .with_context(|| format!("failed to parse `{}`", path.display()))?;
    let unstable = table
      .get("unstable_features")
      .and_then(toml::Value::as_bool)
      .unwrap_or(false);
    Ok(if unstable {
      Self::Nightly
    } else {
      Self::Stable
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn detects_mixed_languages() {
    let dir = tempfile::tempdir().unwrap();
    for file in [
      "Cargo.toml",
      "src/lib.rs",
      "cpp/a.cc",
      "c/b.c",
      "build/gen.cpp",
    ] {
      let path = dir.path().join(file);
      std::fs::create_dir_all(path.parent().unwrap()).unwrap();
      std::fs::write(path, "").unwrap();
    }
    let langs: Vec<_> = Language::detect(dir.path()).unwrap().into_iter().collect();
    assert_eq!(langs, [Language::C, Language::Cpp, Language::Rust]);
  }

  #[test]
  fn displays_human_names() {
    assert_eq!(Language::Cpp.to_string(), "C++");
    assert_eq!(Language::Rust.to_string(), "Rust");
  }

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

  #[test]
  fn follows_unstable_features() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("rustfmt.toml");
    assert_eq!(
      RustfmtChannel::from_path(dir.path()).unwrap(),
      RustfmtChannel::Stable
    );
    std::fs::write(&config, "tab_spaces = 2\n").unwrap();
    assert_eq!(
      RustfmtChannel::from_path(dir.path()).unwrap(),
      RustfmtChannel::Stable
    );
    std::fs::write(
      &config,
      "unstable_features = true\nimports_granularity = \"Crate\"\n",
    )
    .unwrap();
    assert_eq!(
      RustfmtChannel::from_path(dir.path()).unwrap(),
      RustfmtChannel::Nightly
    );
  }

  #[test]
  fn hidden_config_is_read() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
      dir.path().join(".rustfmt.toml"),
      "unstable_features = true\n",
    )
    .unwrap();
    assert_eq!(
      RustfmtChannel::from_path(dir.path()).unwrap(),
      RustfmtChannel::Nightly
    );
  }

  #[test]
  fn the_nearest_ancestor_config_wins() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("tests/project");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(
      dir.path().join("rustfmt.toml"),
      "unstable_features = true\n",
    )
    .unwrap();
    assert_eq!(
      RustfmtChannel::from_path(&nested).unwrap(),
      RustfmtChannel::Nightly
    );
    std::fs::write(nested.join("rustfmt.toml"), "edition = \"2024\"\n").unwrap();
    assert_eq!(
      RustfmtChannel::from_path(&nested).unwrap(),
      RustfmtChannel::Stable
    );
  }

  #[test]
  fn invalid_config_is_reported_with_its_path() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("rustfmt.toml"), "tab_spaces = \n").unwrap();
    let err = RustfmtChannel::from_path(dir.path()).unwrap_err();
    assert!(err.to_string().contains("rustfmt.toml"), "{err}");
  }
}
