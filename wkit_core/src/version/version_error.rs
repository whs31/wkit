#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VersionError {
  #[error("`{input}` is not a valid semantic version: {reason}")]
  InvalidSemver { input: String, reason: String },

  #[error("`{input}` is not a valid calendar version (expected YYYY.MM.DD[.N])")]
  InvalidCalver { input: String },
}
