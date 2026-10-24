#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct MmsCheck {
  pub enabled: Option<bool>,
  pub meta: Option<std::path::PathBuf>,
}

impl MmsCheck {
  pub const META: &str = "meta.lua";
}
