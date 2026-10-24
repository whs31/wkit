#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error(transparent)]
  Core(#[from] wkit_core::Error),

  #[error("{file}: {message}")]
  Config { file: String, message: String },

  #[error("no `{}` in `{}` or any parent directory", crate::WorkspaceManifest::FILE, start.display())]
  NotFound { start: std::path::PathBuf },

  #[error("{0}")]
  Message(String),
}

impl Error {
  pub fn config(file: impl Into<String>, message: impl Into<String>) -> Self {
    Self::Config {
      file: file.into(),
      message: message.into(),
    }
  }

  pub fn exit_code(&self) -> Option<i32> {
    match self {
      Self::Core(e) => e.exit_code(),
      Self::Config { .. } | Self::NotFound { .. } => Some(2),
      Self::Message(_) => Some(1),
    }
  }
}

impl From<std::io::Error> for Error {
  fn from(e: std::io::Error) -> Self {
    Self::Core(wkit_core::Error::Output(e))
  }
}
