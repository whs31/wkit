#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CacheEntry {
  pub key: String,
  pub inputs: std::collections::BTreeMap<std::path::PathBuf, String>,
  pub success: bool,
  pub output: String,
}
