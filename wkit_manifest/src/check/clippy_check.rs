#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
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
