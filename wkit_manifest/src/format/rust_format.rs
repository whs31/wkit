#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct RustFormat {
  pub enabled: Option<bool>,
  pub channel: Option<wkit_core::detect::RustfmtChannel>,
  pub args: Vec<String>,
}
