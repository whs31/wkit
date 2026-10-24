#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error(transparent)]
  Core(#[from] wkit_core::Error),

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

  fn help(hint: Option<&str>) -> String {
    hint.map(|h| format!("\n\nhelp: {h}")).unwrap_or_default()
  }

  pub fn exit_code(&self) -> Option<i32> {
    match self {
      Self::Core(e) => e.exit_code(),
      Self::Setup { .. } => None,
    }
  }
}

impl From<std::io::Error> for Error {
  fn from(e: std::io::Error) -> Self {
    Self::Core(wkit_core::Error::Output(e))
  }
}
