#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ClangTidyCheck {
  pub enabled: Option<bool>,
  pub build_dir: Option<std::path::PathBuf>,
  pub dirs: Vec<String>,
  pub strip_flags: Vec<String>,
  pub warnings_as_errors: bool,
  pub cache: bool,
}

impl ClangTidyCheck {
  pub const DEFAULT_DIRS: &[&str] = &["api", "bin", "include", "src", "tests"];
  pub const DEFAULT_STRIP_FLAGS: &[&str] = &["-mno-direct-extern-access"];
}

impl Default for ClangTidyCheck {
  fn default() -> Self {
    Self {
      enabled: None,
      build_dir: None,
      dirs: Self::DEFAULT_DIRS.iter().map(|&s| s.to_owned()).collect(),
      strip_flags: Self::DEFAULT_STRIP_FLAGS
        .iter()
        .map(|&s| s.to_owned())
        .collect(),
      warnings_as_errors: true,
      cache: true,
    }
  }
}
