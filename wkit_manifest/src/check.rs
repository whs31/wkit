mod clang_tidy_check;
mod clippy_check;

pub use self::{clang_tidy_check::ClangTidyCheck, clippy_check::ClippyCheck};
use crate::CustomCheck;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
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
