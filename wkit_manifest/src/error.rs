use std::{
  fmt::Write,
  io,
  path::{Path, PathBuf},
};

use crate::MANIFEST_FILE;

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
  #[error("could not find `{MANIFEST_FILE}` in `{}` or any parent directory", start.display())]
  NotFound { start: PathBuf },

  #[error("failed to read `{}`", path.display())]
  Read {
    path: PathBuf,
    #[source]
    source: io::Error,
  },

  #[error("failed to parse manifest{}", at(path.as_deref()))]
  Parse {
    path: Option<PathBuf>,
    #[source]
    source: Box<toml::de::Error>,
  },

  #[error("invalid manifest{}:{}", at(path.as_deref()), bullets(problems))]
  Invalid {
    path: Option<PathBuf>,
    problems: Vec<String>,
  },

  #[error(
    "this project requires wkit {required}, but wkit {current} is running; run `wkit self-update`"
  )]
  Incompatible {
    required: semver::VersionReq,
    current: semver::Version,
  },
}

fn at(path: Option<&Path>) -> String {
  path
    .map(|p| format!(" at `{}`", p.display()))
    .unwrap_or_default()
}

fn bullets(problems: &[String]) -> String {
  problems.iter().fold(String::new(), |mut out, p| {
    let _ = write!(out, "\n  - {p}");
    out
  })
}
