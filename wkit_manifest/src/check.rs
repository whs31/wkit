mod clang_tidy_check;
mod clippy_check;

pub use self::{clang_tidy_check::ClangTidyCheck, clippy_check::ClippyCheck};
use crate::CustomCheck;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
#[allow(clippy::struct_excessive_bools)]
pub struct CheckSection {
  pub format: bool,
  pub versions: bool,
  pub secrets: bool,
  pub whitespace: bool,
  pub pre_commit: bool,
  pub clippy: ClippyCheck,
  pub clang_tidy: ClangTidyCheck,
  pub custom: Vec<CustomCheck>,
}

impl CheckSection {
  pub const BUILTIN_STAGES: &[&str] = &[
    "format",
    "versions",
    "clippy",
    "clang-tidy",
    "secrets",
    "whitespace",
    "pre-commit",
  ];
}

impl Default for CheckSection {
  fn default() -> Self {
    Self {
      format: true,
      versions: true,
      secrets: true,
      whitespace: true,
      pre_commit: true,
      clippy: ClippyCheck::default(),
      clang_tidy: ClangTidyCheck::default(),
      custom: Vec::new(),
    }
  }
}
