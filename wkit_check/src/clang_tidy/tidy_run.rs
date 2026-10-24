#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TidyRun {
  pub source: std::path::PathBuf,
  pub success: bool,
  pub output: String,

  #[serde(skip)]
  pub cached: bool,
}
