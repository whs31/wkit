use std::path::PathBuf;

use serde::Deserialize;

use crate::task::CustomCheck;

pub const DEFAULT_TIDY_DIRS: &[&str] = &["api", "bin", "include", "src", "tests"];
pub const DEFAULT_STRIP_FLAGS: &[&str] = &["-mno-direct-extern-access"];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CheckSection {
  pub format: bool,
  pub versions: bool,
  pub clippy: ClippyCheck,
  pub clang_tidy: ClangTidyCheck,
  pub custom: Vec<CustomCheck>,
}

impl Default for CheckSection {
  fn default() -> Self {
    Self {
      format: true,
      versions: true,
      clippy: ClippyCheck::default(),
      clang_tidy: ClangTidyCheck::default(),
      custom: Vec::new(),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ClippyCheck {
  pub enabled: Option<bool>,
  pub toolchain: Option<String>,
  pub all_targets: bool,
  pub deny_warnings: bool,
  pub features: Vec<String>,
  pub all_features: bool,
  pub args: Vec<String>,
}

impl Default for ClippyCheck {
  fn default() -> Self {
    Self {
      enabled: None,
      toolchain: None,
      all_targets: true,
      deny_warnings: true,
      features: Vec::new(),
      all_features: false,
      args: Vec::new(),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ClangTidyCheck {
  pub enabled: Option<bool>,
  pub build_dir: Option<PathBuf>,
  pub dirs: Vec<String>,
  pub strip_flags: Vec<String>,
  pub warnings_as_errors: bool,
  pub cache: bool,
}

impl Default for ClangTidyCheck {
  fn default() -> Self {
    Self {
      enabled: None,
      build_dir: None,
      dirs: DEFAULT_TIDY_DIRS.iter().map(|&s| s.to_owned()).collect(),
      strip_flags: DEFAULT_STRIP_FLAGS.iter().map(|&s| s.to_owned()).collect(),
      warnings_as_errors: true,
      cache: true,
    }
  }
}
