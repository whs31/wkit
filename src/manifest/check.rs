use std::collections::BTreeMap;

use smart_default::SmartDefault;

use super::{Command, Task, strings};
use crate::{detect::RustfmtChannel, fs::SourceSet};

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct FormatSection {
  pub rust: RustFormat,
  pub cpp: CppFormat,
  pub qmlformat: ExtraFormat,
  pub black: ExtraFormat,
  pub stylua: ExtraFormat,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct RustFormat {
  pub enabled: Option<bool>,
  pub channel: Option<RustfmtChannel>,
  pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, SmartDefault, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CppFormat {
  pub enabled: Option<bool>,
  #[default(strings(&["c", "cc", "cpp", "cxx", "h", "hh", "hpp", "hxx"]))]
  pub extensions: Vec<String>,
  pub include: Vec<String>,
  #[default(strings(SourceSet::DEFAULT_EXCLUDES))]
  pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ExtraFormat {
  pub enabled: Option<bool>,
  pub extensions: Option<Vec<String>>,
  pub include: Vec<String>,
  pub exclude: Option<Vec<String>>,
  pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, SmartDefault, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
#[allow(clippy::struct_excessive_bools)]
pub struct CheckSection {
  #[default(true)]
  pub format: bool,
  #[default(true)]
  pub versions: bool,
  #[default(true)]
  pub secrets: bool,
  #[default(true)]
  pub whitespace: bool,
  #[default(true)]
  pub pre_commit: bool,
  pub clippy: ClippyCheck,
  pub clang_tidy: ClangTidyCheck,
  pub qmllint: QmllintCheck,
  pub mms: MmsCheck,
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
    "qmllint",
    "mms",
  ];
}

#[derive(Debug, Clone, PartialEq, Eq, SmartDefault, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ClippyCheck {
  pub enabled: Option<bool>,
  pub toolchain: Option<String>,
  #[default(true)]
  pub all_targets: bool,
  #[default(true)]
  pub deny_warnings: bool,
  pub features: Vec<String>,
  pub all_features: bool,
  pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, SmartDefault, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ClangTidyCheck {
  pub enabled: Option<bool>,
  pub build_dir: Option<std::path::PathBuf>,
  #[default(strings(&["api", "bin", "include", "src", "tests"]))]
  pub dirs: Vec<String>,
  #[default(strings(&["-mno-direct-extern-access"]))]
  pub strip_flags: Vec<String>,
  #[default(true)]
  pub warnings_as_errors: bool,
  #[default(true)]
  pub cache: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct QmllintCheck {
  pub enabled: bool,
  pub include: Vec<String>,
  pub exclude: Option<Vec<String>>,
  pub args: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct MmsCheck {
  pub enabled: Option<bool>,
  pub meta: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CustomCheck {
  pub name: String,
  pub run: Command,
  pub description: Option<String>,
  pub cwd: Option<std::path::PathBuf>,
  #[serde(default)]
  pub env: BTreeMap<String, String>,
}

impl CustomCheck {
  pub fn task(&self) -> Task {
    Task {
      run: self.run.clone(),
      description: self.description.clone(),
      cwd: self.cwd.clone(),
      env: self.env.clone(),
    }
  }
}
