use wkit_core::ProcessBuilder;

use super::Need;

#[derive(Debug, Clone)]
pub struct Probe {
  pub name: String,
  pub command: ProcessBuilder,
  pub need: Need,
  pub purpose: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
  Version(String),
  Missing(String),
}

impl Probe {
  pub fn new(name: &str, command: ProcessBuilder, need: Need, purpose: &str) -> Self {
    Self {
      name: name.to_owned(),
      command,
      need,
      purpose: purpose.to_owned(),
    }
  }

  pub fn system(name: &str, need: Need, purpose: &str) -> Self {
    Self::new(
      name,
      ProcessBuilder::new(name).arg("--version"),
      need,
      purpose,
    )
  }

  pub fn run(&self) -> Found {
    match self.command.output_unchecked() {
      Err(wkit_core::Error::ProgramNotFound { .. }) => Found::Missing("not found".to_owned()),
      Err(e) => Found::Missing(e.to_string()),
      Ok(output) => {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.success() {
          Found::Version(
            Self::version(&stdout)
              .or_else(|| Self::version(&stderr))
              .unwrap_or_else(|| "found".to_owned()),
          )
        } else {
          let reason = stderr
            .lines()
            .chain(stdout.lines())
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("failed")
            .trim_start_matches("error: ");
          Found::Missing(reason.to_owned())
        }
      }
    }
  }

  pub fn version(text: &str) -> Option<String> {
    static VERSION: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(r"\b\d+\.\d+(?:\.\d+)?(?:[-+~][0-9A-Za-z.+~-]*[0-9A-Za-z])?\b")
        .expect("valid")
    });
    VERSION.find(text).map(|m| m.as_str().to_owned())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn versions_are_picked_out_of_banner_lines() {
    for (banner, version) in [
      ("git version 2.51.0\n", "2.51.0"),
      ("Conan version 2.21.0", "2.21.0"),
      ("cmake version 4.1.2\n\nCMake suite maintained", "4.1.2"),
      (
        "rustfmt 1.8.0-nightly (6d6a08cf59 2025-09-04)",
        "1.8.0-nightly",
      ),
      ("qmlformat 6.12.0", "6.12.0"),
      ("1.13.1\n", "1.13.1"),
      ("cargo-deb 3.6.0", "3.6.0"),
    ] {
      assert_eq!(Probe::version(banner).as_deref(), Some(version), "{banner}");
    }
    assert_eq!(Probe::version("no digits"), None);
  }

  #[test]
  fn missing_and_failing_programs_are_reported() {
    let missing = Probe::system("wkit-surely-missing-tool", Need::Optional, "");
    assert_eq!(missing.run(), Found::Missing("not found".into()));
    let failing = Probe::new(
      "sh",
      ProcessBuilder::shell("echo 'error: no such command' >&2; exit 101"),
      Need::Required,
      "",
    );
    assert_eq!(failing.run(), Found::Missing("no such command".into()));
    let found = Probe::new(
      "sh",
      ProcessBuilder::shell("echo tool 1.2.3"),
      Need::Required,
      "",
    );
    assert_eq!(found.run(), Found::Version("1.2.3".into()));
  }
}
