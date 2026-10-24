#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CppFormat {
  pub enabled: Option<bool>,
  pub extensions: Vec<String>,
  pub include: Vec<String>,
  pub exclude: Vec<String>,
}

impl CppFormat {
  pub const DEFAULT_EXTENSIONS: &[&str] = &["c", "cc", "cpp", "cxx", "h", "hh", "hpp", "hxx"];
}

impl Default for CppFormat {
  fn default() -> Self {
    Self {
      enabled: None,
      extensions: Self::DEFAULT_EXTENSIONS
        .iter()
        .map(|&s| s.to_owned())
        .collect(),
      include: Vec::new(),
      exclude: wkit_core::fs::SourceSet::DEFAULT_EXCLUDES
        .iter()
        .map(|&s| s.to_owned())
        .collect(),
    }
  }
}
