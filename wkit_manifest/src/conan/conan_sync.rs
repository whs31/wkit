#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ConanSync {
  pub remote: Option<String>,
  pub check: bool,
  pub force: bool,
}

impl Default for ConanSync {
  fn default() -> Self {
    Self {
      remote: None,
      check: true,
      force: false,
    }
  }
}
