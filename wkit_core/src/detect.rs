use std::{collections::BTreeSet, fmt, path::Path};

use serde::Deserialize;

use crate::{Error, Language, Result, fs::SourceSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildSystem {
  Cargo,
  Conan,
  #[serde(rename = "cmake")]
  CMake,
}

impl fmt::Display for BuildSystem {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(match self {
      Self::Cargo => "cargo",
      Self::Conan => "conan",
      Self::CMake => "cmake",
    })
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RustfmtChannel {
  Stable,
  Nightly,
}

pub const RUSTFMT_CONFIGS: &[&str] = &["rustfmt.toml", ".rustfmt.toml"];

pub fn build_system(root: &Path) -> Option<BuildSystem> {
  [
    ("conanfile.py", BuildSystem::Conan),
    ("Cargo.toml", BuildSystem::Cargo),
    ("CMakeLists.txt", BuildSystem::CMake),
  ]
  .into_iter()
  .find(|(file, _)| root.join(file).is_file())
  .map(|(_, system)| system)
}

pub fn languages(root: &Path) -> Result<BTreeSet<Language>> {
  let mut found = BTreeSet::new();
  if root.join("Cargo.toml").is_file() {
    found.insert(Language::Rust);
  }
  for file in SourceSet::new(root).collect()? {
    if let Some(lang) = file
      .extension()
      .and_then(|e| e.to_str())
      .and_then(Language::from_extension)
    {
      found.insert(lang);
    }
  }
  Ok(found)
}

pub fn rustfmt_channel(root: &Path) -> Result<RustfmtChannel> {
  for name in RUSTFMT_CONFIGS {
    let path = root.join(name);
    let Some(text) = crate::fs::read_optional(&path)? else {
      continue;
    };
    let table: toml::Table = toml::from_str(&text).map_err(|source| Error::Toml {
      path,
      source: Box::new(source),
    })?;
    let unstable = table
      .get("unstable_features")
      .and_then(toml::Value::as_bool)
      .unwrap_or(false);
    return Ok(if unstable {
      RustfmtChannel::Nightly
    } else {
      RustfmtChannel::Stable
    });
  }
  Ok(RustfmtChannel::Stable)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
  }

  #[test]
  fn rustfmt_channel_follows_unstable_features() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(rustfmt_channel(dir.path()).unwrap(), RustfmtChannel::Stable);
    write(dir.path(), "rustfmt.toml", "tab_spaces = 2\n");
    assert_eq!(rustfmt_channel(dir.path()).unwrap(), RustfmtChannel::Stable);
    write(
      dir.path(),
      "rustfmt.toml",
      "unstable_features = true\nimports_granularity = \"Crate\"\n",
    );
    assert_eq!(
      rustfmt_channel(dir.path()).unwrap(),
      RustfmtChannel::Nightly
    );
  }

  #[test]
  fn hidden_rustfmt_config_is_read() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), ".rustfmt.toml", "unstable_features = true\n");
    assert_eq!(
      rustfmt_channel(dir.path()).unwrap(),
      RustfmtChannel::Nightly
    );
  }

  #[test]
  fn conan_wins_over_cargo_and_cmake() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(build_system(dir.path()), None);
    write(dir.path(), "CMakeLists.txt", "");
    assert_eq!(build_system(dir.path()), Some(BuildSystem::CMake));
    write(dir.path(), "Cargo.toml", "");
    assert_eq!(build_system(dir.path()), Some(BuildSystem::Cargo));
    write(dir.path(), "conanfile.py", "");
    assert_eq!(build_system(dir.path()), Some(BuildSystem::Conan));
  }

  #[test]
  fn detects_mixed_languages() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Cargo.toml", "");
    write(dir.path(), "src/lib.rs", "");
    write(dir.path(), "cpp/a.cc", "");
    write(dir.path(), "c/b.c", "");
    write(dir.path(), "build/gen.cpp", "");
    let langs: Vec<_> = languages(dir.path()).unwrap().into_iter().collect();
    assert_eq!(langs, [Language::C, Language::Cpp, Language::Rust]);
  }
}
