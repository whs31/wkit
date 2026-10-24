use serde::Deserialize;
use wkit_core::{detect::RustfmtChannel, fs::DEFAULT_EXCLUDES};

pub const DEFAULT_CPP_EXTENSIONS: &[&str] = &["c", "cc", "cpp", "cxx", "h", "hh", "hpp", "hxx"];

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct FormatSection {
  pub rust: RustFormat,
  pub cpp: CppFormat,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct RustFormat {
  pub enabled: Option<bool>,
  pub channel: Option<RustfmtChannel>,
  pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CppFormat {
  pub enabled: Option<bool>,
  pub extensions: Vec<String>,
  pub include: Vec<String>,
  pub exclude: Vec<String>,
}

impl Default for CppFormat {
  fn default() -> Self {
    Self {
      enabled: None,
      extensions: DEFAULT_CPP_EXTENSIONS
        .iter()
        .map(|&s| s.to_owned())
        .collect(),
      include: Vec::new(),
      exclude: DEFAULT_EXCLUDES.iter().map(|&s| s.to_owned()).collect(),
    }
  }
}
