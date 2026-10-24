#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error(transparent)]
  Core(#[from] wkit_core::Error),

  #[error(transparent)]
  Version(#[from] wkit_version::Error),

  #[error("failed to parse `{}`", path.display())]
  Json {
    path: std::path::PathBuf,
    #[source]
    source: serde_json::Error,
  },

  #[error("{0}")]
  Message(String),
}

impl Error {
  pub fn exit_code(&self) -> Option<i32> {
    match self {
      Self::Core(e) => e.exit_code(),
      Self::Version(e) => e.exit_code(),
      Self::Json { .. } | Self::Message(_) => None,
    }
  }
}

impl From<std::io::Error> for Error {
  fn from(e: std::io::Error) -> Self {
    Self::Core(wkit_core::Error::Output(e))
  }
}
