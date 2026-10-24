#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct QmllintCheck {
  pub enabled: bool,
  pub include: Vec<String>,
  pub exclude: Option<Vec<String>>,
  pub args: Vec<String>,
}
