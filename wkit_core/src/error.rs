use std::{io, path::PathBuf, process::ExitStatus};

use crate::version::VersionError;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("failed to read `{}`", path.display())]
  Read {
    path: PathBuf,
    #[source]
    source: io::Error,
  },

  #[error("failed to write `{}`", path.display())]
  Write {
    path: PathBuf,
    #[source]
    source: io::Error,
  },

  #[error("failed to parse `{}`", path.display())]
  Toml {
    path: PathBuf,
    #[source]
    source: Box<toml::de::Error>,
  },

  #[error("failed to walk `{}`", root.display())]
  Walk {
    root: PathBuf,
    #[source]
    source: ignore::Error,
  },

  #[error("could not find `{program}` in PATH")]
  ProgramNotFound { program: String },

  #[error("could not execute `{command}`")]
  Spawn {
    command: String,
    #[source]
    source: io::Error,
  },

  #[error("process didn't exit successfully: `{command}` ({status}){}", render_stderr(.stderr))]
  ProcessFailed {
    command: String,
    status: ExitStatus,
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
}

fn render_stderr(stderr: &str) -> String {
  let stderr = stderr.trim_end();
  if stderr.is_empty() {
    String::new()
  } else {
    format!("\n--- stderr\n{stderr}")
  }
}
