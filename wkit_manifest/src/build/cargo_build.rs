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
