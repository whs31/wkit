#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct IndexEntry {
  pub vers: semver::Version,

  #[serde(default)]
  pub yanked: bool,
}
