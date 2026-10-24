#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error(transparent)]
  Core(#[from] wkit_core::Error),

  #[error(transparent)]
  Build(#[from] wkit_build::Error),

  #[error(transparent)]
  Version(#[from] wkit_version::Error),

  #[error("`{tool}` failed ({status})\n\nnote: the full log is in `{}`", log.display())]
  ToolFailed {
    tool: String,
    status: std::process::ExitStatus,
    log: std::path::PathBuf,
  },

  #[error("{message}{}", Self::help(hint.as_deref()))]
  Setup {
    message: String,
    hint: Option<String>,
  },
}

impl Error {
  pub fn setup(message: impl Into<String>, hint: Option<String>) -> Self {
    Self::Setup {
      message: message.into(),
      hint,
    }
  }

  pub fn exit_code(&self) -> Option<i32> {
    match self {
      Self::Core(e) => e.exit_code(),
      Self::Build(e) => e.exit_code(),
      Self::Version(e) => e.exit_code(),
      Self::ToolFailed { status, .. } => status.code(),
      Self::Setup { .. } => None,
    }
  }

  fn help(hint: Option<&str>) -> String {
    hint.map(|h| format!("\n\nhelp: {h}")).unwrap_or_default()
  }
}

impl From<std::io::Error> for Error {
  fn from(e: std::io::Error) -> Self {
    Self::Core(wkit_core::Error::Output(e))
  }
}
