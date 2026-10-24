use std::collections::BTreeMap;

use smart_default::SmartDefault;

use crate::{Language, ProcessBuilder, detect::BuildSystem};

#[derive(Debug, Clone, PartialEq, Eq, SmartDefault, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct BuildSection {
  pub system: Option<BuildSystem>,
  #[default(true)]
  pub link_compile_commands: bool,
  pub conan: ConanBuild,
  pub cargo: CargoBuild,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, derive_more::Display)]
pub enum BuildType {
  #[default]
  #[serde(alias = "debug")]
  Debug,
  #[serde(alias = "release")]
  Release,
  #[serde(alias = "relwithdebinfo", alias = "rel-with-deb-info")]
  RelWithDebInfo,
  #[serde(alias = "minsizerel", alias = "min-size-rel")]
  MinSizeRel,
}

impl BuildType {
  pub const ALL: [Self; 4] = [
    Self::Debug,
    Self::Release,
    Self::RelWithDebInfo,
    Self::MinSizeRel,
  ];

  pub fn is_optimized(self) -> bool {
    self != Self::Debug
  }
}

impl std::str::FromStr for BuildType {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let wanted = s.replace(['-', '_'], "");
    Self::ALL
      .into_iter()
      .find(|t| t.to_string().eq_ignore_ascii_case(&wanted))
      .ok_or_else(|| {
        format!("unknown build type `{s}` (expected Debug, Release, RelWithDebInfo or MinSizeRel)")
      })
  }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Bin {
  pub name: String,
  pub path: Option<String>,
  pub package: Option<String>,
  #[serde(default)]
  pub args: Vec<String>,
  #[serde(default)]
  pub env: BTreeMap<String, String>,
  pub cwd: Option<std::path::PathBuf>,
}

impl Bin {
  pub fn resolve_path(&self, build_type: BuildType) -> Option<std::path::PathBuf> {
    self
      .path
      .as_deref()
      .map(|template| expand(template, build_type).into())
  }
}

pub fn expand(template: &str, build_type: BuildType) -> String {
  template
    .replace("{build-type}", &build_type.to_string())
    .replace("{exe}", std::env::consts::EXE_SUFFIX)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CargoBuild {
  pub toolchain: Option<String>,
  pub package: Option<String>,
  pub features: Vec<String>,
  pub all_features: bool,
  pub no_default_features: bool,
  pub locked: bool,
  pub args: Vec<String>,
}

impl CargoBuild {
  pub fn command(&self, root: &std::path::Path, subcommand: &[&str]) -> ProcessBuilder {
    let mut cargo = ProcessBuilder::new("cargo").cwd(root);
    if let Some(toolchain) = &self.toolchain {
      cargo = cargo.arg(format!("+{toolchain}"));
    }
    cargo = cargo.args(subcommand);
    if self.all_features {
      cargo = cargo.arg("--all-features");
    } else if !self.features.is_empty() {
      cargo = cargo.arg("--features").arg(self.features.join(","));
    }
    if self.no_default_features {
      cargo = cargo.arg("--no-default-features");
    }
    if self.locked {
      cargo = cargo.arg("--locked");
    }
    cargo
  }
}

#[derive(Debug, Clone, PartialEq, Eq, SmartDefault, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ConanBuild {
  pub profile_host: Option<String>,
  pub profile_build: Option<String>,
  pub settings: BTreeMap<String, String>,
  pub conf: BTreeMap<String, ConanValue>,
  #[default(true)]
  pub build_missing: bool,
  pub test_option: Option<String>,
  pub workspace: bool,
  pub args: Vec<String>,
}

impl ConanBuild {
  pub const TEST_OPTIONS: [&str; 6] = [
    "test",
    "tests",
    "with_tests",
    "build_tests",
    "enable_tests",
    "testing",
  ];

  pub fn setting_args(&self) -> Vec<String> {
    self
      .settings
      .iter()
      .flat_map(|(key, value)| ["-s".to_owned(), format!("{key}={value}")])
      .collect()
  }

  pub fn conf_args(&self) -> Vec<String> {
    self
      .conf
      .iter()
      .flat_map(|(key, value)| ["-c".to_owned(), format!("{key}={value}")])
      .collect()
  }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, derive_more::Display)]
#[serde(untagged)]
pub enum ConanValue {
  #[display("{}", if *_0 { "True" } else { "False" })]
  Bool(bool),
  Int(i64),
  String(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ConanSection {
  pub sync: ConanSync,
}

#[derive(Debug, Clone, PartialEq, Eq, SmartDefault, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ConanSync {
  pub remote: Option<String>,
  #[default(true)]
  pub check: bool,
  pub force: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct TestSection {
  pub junit_dir: Option<std::path::PathBuf>,
  #[serde(rename = "suite")]
  pub suites: Vec<TestSuite>,
}

impl TestSection {
  pub fn junit_dir(&self) -> &std::path::Path {
    self
      .junit_dir
      .as_deref()
      .unwrap_or(std::path::Path::new("build/junit"))
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, derive_more::Display)]
#[serde(rename_all = "lowercase")]
#[display(rename_all = "lowercase")]
pub enum TestRunner {
  Cargo,
  Nextest,
  Ctest,
  Catch2,
}

impl TestRunner {
  pub fn needs_build_dir(self) -> bool {
    matches!(self, Self::Ctest | Self::Catch2)
  }

  pub fn serves(self, language: Language) -> bool {
    match self {
      Self::Cargo | Self::Nextest => language == Language::Rust,
      Self::Ctest | Self::Catch2 => language.is_c_family(),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TestSuite {
  pub name: Option<String>,
  pub runner: TestRunner,
  pub profile: Option<String>,
  pub binary: Option<String>,
  pub build_dir: Option<String>,
  pub filter: Option<String>,
  #[serde(default)]
  pub args: Vec<String>,
  #[serde(default)]
  pub env: BTreeMap<String, String>,
}

impl TestSuite {
  pub fn display_name(&self) -> String {
    self
      .name
      .clone()
      .or_else(|| self.binary.clone())
      .unwrap_or_else(|| self.runner.to_string())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn build_types_parse_loosely_and_display_cmake_names() {
    assert_eq!("release".parse(), Ok(BuildType::Release));
    assert_eq!("rel-with-deb-info".parse(), Ok(BuildType::RelWithDebInfo));
    assert_eq!("MINSIZEREL".parse(), Ok(BuildType::MinSizeRel));
    assert!("fast".parse::<BuildType>().is_err());
    assert_eq!(BuildType::RelWithDebInfo.to_string(), "RelWithDebInfo");
  }

  #[test]
  fn bin_paths_expand_build_type_and_exe_suffix() {
    let bin = Bin {
      name: "mms".into(),
      path: Some("build/{build-type}/bin/mms{exe}".into()),
      ..Bin::default()
    };
    let expected = format!(
      "build/RelWithDebInfo/bin/mms{}",
      std::env::consts::EXE_SUFFIX
    );
    assert_eq!(
      bin.resolve_path(BuildType::RelWithDebInfo),
      Some(expected.into())
    );
    assert_eq!(
      Bin { path: None, ..bin }.resolve_path(BuildType::Debug),
      None
    );
  }

  #[test]
  fn cargo_command_carries_the_feature_selection() {
    let build = CargoBuild {
      toolchain: Some("1.88".into()),
      features: vec!["a".into(), "b".into()],
      no_default_features: true,
      locked: true,
      ..CargoBuild::default()
    };
    assert_eq!(
      build
        .command(std::path::Path::new("/r"), &["test", "--workspace"])
        .to_string(),
      "cargo +1.88 test --workspace --features a,b --no-default-features --locked"
    );
  }

  #[test]
  fn conan_settings_and_conf_become_arguments() {
    let conan: ConanBuild = toml::from_str(
      r#"
      settings = { "compiler.cppstd" = "gnu23" }
      conf = { "tools.build:jobs" = 4, "core:non_interactive" = true, "user:x" = "y" }
      "#,
    )
    .unwrap();
    assert_eq!(conan.setting_args(), ["-s", "compiler.cppstd=gnu23"]);
    assert_eq!(
      conan.conf_args(),
      [
        "-c",
        "core:non_interactive=True",
        "-c",
        "tools.build:jobs=4",
        "-c",
        "user:x=y"
      ]
    );
    assert_eq!(conan.test_option, None);
    assert!(conan.build_missing);
  }

  #[test]
  fn runners_serve_their_language_family() {
    assert!(TestRunner::Nextest.serves(Language::Rust));
    assert!(!TestRunner::Nextest.serves(Language::Cpp));
    assert!(TestRunner::Catch2.serves(Language::C));
    assert_eq!(TestRunner::Catch2.to_string(), "catch2");
    assert!(TestRunner::Ctest.needs_build_dir() && !TestRunner::Cargo.needs_build_dir());
  }
}
