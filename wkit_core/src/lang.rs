use std::fmt;

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
  C,
  #[serde(alias = "c++", alias = "cxx")]
  Cpp,
  #[serde(alias = "rs")]
  Rust,
}

impl Language {
  pub fn name(self) -> &'static str {
    match self {
      Self::C => "C",
      Self::Cpp => "C++",
      Self::Rust => "Rust",
    }
  }

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
}

impl fmt::Display for Language {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.name())
  }
}
