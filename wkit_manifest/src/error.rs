use crate::MANIFEST_FILE;

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
  #[error("could not find `{MANIFEST_FILE}` in `{}` or any parent directory", start.display())]
  NotFound { start: std::path::PathBuf },

  #[error("failed to read `{}`", path.display())]
  Read {
    path: std::path::PathBuf,
    #[source]
    source: std::io::Error,
  },

  #[error("failed to parse manifest{}", Self::location(path.as_deref()))]
  Parse {
    path: Option<std::path::PathBuf>,
    #[source]
    source: Box<toml::de::Error>,
  },

  #[error("invalid manifest{}:{}", Self::location(path.as_deref()), Self::bullets(problems))]
  Invalid {
    path: Option<std::path::PathBuf>,
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

impl ManifestError {
  fn location(path: Option<&std::path::Path>) -> String {
    path
      .map(|p| format!(" at `{}`", p.display()))
      .unwrap_or_default()
  }

  fn bullets(problems: &[String]) -> String {
    problems.iter().fold(String::new(), |mut out, p| {
      out.push_str("\n  - ");
      out.push_str(p);
      out
    })
  }
}
