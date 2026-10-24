#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct WindowsPackage {
  pub generator: WindowsGenerator,
  pub icon: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize, derive_more::Display)]
#[serde(rename_all = "lowercase")]
pub enum WindowsGenerator {
  #[default]
  #[display("NSIS64")]
  Nsis,

  #[display("WIX")]
  Wix,

  #[display("ZIP")]
  Zip,
}
