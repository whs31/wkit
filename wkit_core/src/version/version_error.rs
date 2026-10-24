#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VersionError {
  #[error("`{input}` is not a valid semantic version: {reason}")]
  InvalidSemver { input: String, reason: String },

  #[error("`{input}` is not a valid calendar version (expected YYYY.MM.DD[.N])")]
  InvalidCalver { input: String },

  #[error("`{id}` is not a valid prerelease identifier: {reason}")]
  InvalidPrerelease { id: String, reason: String },

  #[error("{from} -> {to} would not increase the version")]
  NotIncreasing { from: String, to: String },

  #[error("calendar versions have no prereleases")]
  CalverPrerelease,
}
