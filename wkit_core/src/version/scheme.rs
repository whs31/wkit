#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, derive_more::Display)]
#[serde(rename_all = "lowercase")]
#[display(rename_all = "lowercase")]
pub enum Scheme {
  #[default]
  SemVer,

  CalVer,
}
