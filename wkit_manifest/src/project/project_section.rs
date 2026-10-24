#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProjectSection {
  pub name: Option<String>,
  pub description: Option<String>,
  pub languages: std::collections::BTreeSet<wkit_core::Language>,
  pub default_run: Option<String>,

  #[serde(default)]
  pub wkit: Option<semver::VersionReq>,
}
