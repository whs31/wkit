use super::TestRunner;

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
  pub env: std::collections::BTreeMap<String, String>,
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
