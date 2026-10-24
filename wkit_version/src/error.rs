use wkit_core::version::VersionError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error(transparent)]
  Core(#[from] wkit_core::Error),

  #[error("could not find a version in `{}`", path.display())]
  NotFound { path: std::path::PathBuf },

  #[error("`{}` has an unusable version", path.display())]
  Invalid {
    path: std::path::PathBuf,
    #[source]
    source: VersionError,
  },
}

impl Error {
  pub fn exit_code(&self) -> Option<i32> {
    match self {
      Self::Core(e) => e.exit_code(),
      Self::NotFound { .. } | Self::Invalid { .. } => None,
    }
  }
}
