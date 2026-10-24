use std::{collections::BTreeMap, fmt, path::PathBuf, str::FromStr};

use serde::Deserialize;
use wkit_core::detect::BuildSystem;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct BuildSection {
  pub system: Option<BuildSystem>,
  pub build_type: BuildType,
  pub conan: ConanBuild,
  pub cargo: CargoBuild,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
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

  pub fn as_str(self) -> &'static str {
    match self {
      Self::Debug => "Debug",
      Self::Release => "Release",
      Self::RelWithDebInfo => "RelWithDebInfo",
      Self::MinSizeRel => "MinSizeRel",
    }
  }

  pub fn is_optimized(self) -> bool {
    self != Self::Debug
  }
}

impl fmt::Display for BuildType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

impl FromStr for BuildType {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let wanted = s.replace(['-', '_'], "");
    Self::ALL
      .into_iter()
      .find(|t| t.as_str().eq_ignore_ascii_case(&wanted))
      .ok_or_else(|| {
        format!("unknown build type `{s}` (expected Debug, Release, RelWithDebInfo or MinSizeRel)")
      })
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum ConanValue {
  Bool(bool),
  Int(i64),
  String(String),
}

impl fmt::Display for ConanValue {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Bool(true) => f.write_str("True"),
      Self::Bool(false) => f.write_str("False"),
      Self::Int(i) => write!(f, "{i}"),
      Self::String(s) => f.write_str(s),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ConanBuild {
  pub profile_host: Option<String>,
  pub profile_build: Option<String>,
  pub settings: BTreeMap<String, String>,
  pub options: BTreeMap<String, ConanValue>,
  pub conf: BTreeMap<String, ConanValue>,
  pub build_missing: bool,
  pub test_option: Option<String>,
  pub workspace: bool,
  pub args: Vec<String>,
}

impl Default for ConanBuild {
  fn default() -> Self {
    Self {
      profile_host: None,
      profile_build: None,
      settings: BTreeMap::new(),
      options: BTreeMap::new(),
      conf: BTreeMap::new(),
      build_missing: true,
      test_option: Some("test".to_owned()),
      workspace: false,
      args: Vec::new(),
    }
  }
}

impl ConanBuild {
  pub fn option_args(&self) -> Vec<String> {
    self
      .options
      .iter()
      .flat_map(|(key, value)| {
        let scoped = if key.contains(':') {
          key.clone()
        } else {
          format!("&:{key}")
        };
        ["-o".to_owned(), format!("{scoped}={value}")]
      })
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Bin {
  pub name: String,
  pub path: Option<String>,
  pub package: Option<String>,
  #[serde(default)]
  pub args: Vec<String>,
  #[serde(default)]
  pub env: BTreeMap<String, String>,
  pub cwd: Option<PathBuf>,
}

impl Bin {
  pub fn resolve_path(&self, build_type: BuildType) -> Option<PathBuf> {
    let template = self.path.as_deref()?;
    Some(
      template
        .replace("{build-type}", build_type.as_str())
        .replace("{exe}", std::env::consts::EXE_SUFFIX)
        .into(),
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn build_type_parses_loosely() {
    assert_eq!("release".parse(), Ok(BuildType::Release));
    assert_eq!("rel-with-deb-info".parse(), Ok(BuildType::RelWithDebInfo));
    assert_eq!("MINSIZEREL".parse(), Ok(BuildType::MinSizeRel));
    assert!("fast".parse::<BuildType>().is_err());
  }

  #[test]
  fn conan_options_are_consumer_scoped_with_python_bools() {
    let conan: ConanBuild = toml::from_str(
      r#"
      options = { test = true, with_map = false, "qt/*:shared" = true, jobs = 4 }
      conf = { "tools.build:jobs" = 4, "core:non_interactive" = true }
      "#,
    )
    .unwrap();
    assert_eq!(
      conan.option_args(),
      [
        "-o",
        "&:jobs=4",
        "-o",
        "qt/*:shared=True",
        "-o",
        "&:test=True",
        "-o",
        "&:with_map=False"
      ]
    );
    assert_eq!(
      conan.conf_args(),
      [
        "-c",
        "core:non_interactive=True",
        "-c",
        "tools.build:jobs=4"
      ]
    );
    assert_eq!(conan.test_option.as_deref(), Some("test"));
    assert!(conan.build_missing);
  }

  #[test]
  fn bin_path_templates() {
    let bin = Bin {
      name: "mms".into(),
      path: Some("build/{build-type}/bin/mms{exe}".into()),
      package: None,
      args: vec![],
      env: BTreeMap::new(),
      cwd: None,
    };
    let expected = format!(
      "build/RelWithDebInfo/bin/mms{}",
      std::env::consts::EXE_SUFFIX
    );
    assert_eq!(
      bin.resolve_path(BuildType::RelWithDebInfo),
      Some(expected.into())
    );
  }
}
