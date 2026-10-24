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
}
