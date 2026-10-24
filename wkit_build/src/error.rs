#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error(transparent)]
  Core(#[from] wkit_core::Error),

  #[error("{}{}", Self::headline(package, errors), Self::log_note(log.as_deref()))]
  Failed {
    package: String,
    errors: Vec<String>,
    log: Option<std::path::PathBuf>,
  },

  #[error("{0}")]
  Message(String),
}

impl Error {
  pub fn exit_code(&self) -> Option<i32> {
    match self {
      Self::Core(e) => e.exit_code(),
      Self::Failed { .. } | Self::Message(_) => None,
    }
  }

  fn headline(package: &str, errors: &[String]) -> String {
    if errors.is_empty() {
      format!("could not compile `{package}` due to previous errors")
    } else {
      format!("could not build `{package}`:\n  {}", errors.join("\n  "))
    }
  }

  fn log_note(log: Option<&std::path::Path>) -> String {
    log
      .map(|log| format!("\n\nnote: the full build log is in `{}`", log.display()))
      .unwrap_or_default()
  }
}

impl From<std::io::Error> for Error {
  fn from(e: std::io::Error) -> Self {
    Self::Core(wkit_core::Error::Output(e))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn failures_point_at_the_log() {
    let compile = Error::Failed {
      package: "geodesy v2.4.1".into(),
      errors: vec![],
      log: Some("build/wkit-build.log".into()),
    };
    assert_eq!(
      compile.to_string(),
      "could not compile `geodesy v2.4.1` due to previous errors\n\nnote: the full build log is in `build/wkit-build.log`"
    );
    let resolve = Error::Failed {
      package: "app".into(),
      errors: vec!["Package 'x/1.0' not resolved".into()],
      log: None,
    };
    assert_eq!(
      resolve.to_string(),
      "could not build `app`:\n  Package 'x/1.0' not resolved"
    );
  }
}
