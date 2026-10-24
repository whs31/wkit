use std::{collections::BTreeMap, fmt, path::PathBuf};

use serde::Deserialize;
use wkit_core::Language;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct TestSection {
  pub junit_dir: Option<PathBuf>,
  #[serde(rename = "suite")]
  pub suites: Vec<TestSuite>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TestRunner {
  Cargo,
  Nextest,
  Ctest,
  Catch2,
}

impl TestRunner {
  pub fn serves(self, language: Language) -> bool {
    match self {
      Self::Cargo | Self::Nextest => language == Language::Rust,
      Self::Ctest | Self::Catch2 => language.is_c_family(),
    }
  }
}

impl fmt::Display for TestRunner {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(match self {
      Self::Cargo => "cargo",
      Self::Nextest => "nextest",
      Self::Ctest => "ctest",
      Self::Catch2 => "catch2",
    })
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
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
