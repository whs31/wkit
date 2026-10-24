use crate::version::VersionError;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("failed to read `{}`", path.display())]
  Read {
    path: std::path::PathBuf,
    #[source]
    source: std::io::Error,
  },

  #[error("failed to write `{}`", path.display())]
  Write {
    path: std::path::PathBuf,
    #[source]
    source: std::io::Error,
  },

  #[error("failed to parse `{}`", path.display())]
  Toml {
    path: std::path::PathBuf,
    #[source]
    source: Box<toml::de::Error>,
  },

  #[error("failed to walk `{}`", root.display())]
  Walk {
    root: std::path::PathBuf,
    #[source]
    source: ignore::Error,
  },

  #[error("could not find `{program}` in PATH")]
  ProgramNotFound { program: String },

  #[error("could not execute `{command}`")]
  Spawn {
    command: String,
    #[source]
    source: std::io::Error,
  },

  #[error("process didn't exit successfully: `{command}` ({status}){}", Self::stderr_section(.stderr))]
  ProcessFailed {
    command: String,
    status: std::process::ExitStatus,
    stderr: String,
  },

  #[error(transparent)]
  Version(#[from] VersionError),
}

impl Error {
  pub fn exit_code(&self) -> Option<i32> {
    match self {
      Self::ProcessFailed { status, .. } => status.code(),
      _ => None,
    }
  }

  fn stderr_section(stderr: &str) -> String {
    let stderr = stderr.trim_end();
    if stderr.is_empty() {
      String::new()
    } else {
      format!("\n--- stderr\n{stderr}")
    }
  }
}
