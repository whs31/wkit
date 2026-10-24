use wkit_core::{ProcessBuilder, Shell};
use wkit_manifest::check::ClippyCheck;

use crate::{Check, CheckOptions, Outcome, Result};

#[derive(Debug, Clone)]
pub struct Clippy {
  root: std::path::PathBuf,
  config: ClippyCheck,
}

impl Clippy {
  pub fn new(root: impl Into<std::path::PathBuf>, config: ClippyCheck) -> Self {
    Self {
      root: root.into(),
      config,
    }
  }

  pub fn command(&self) -> ProcessBuilder {
    let config = &self.config;
    let mut cargo = ProcessBuilder::new("cargo").cwd(&self.root);
    if let Some(toolchain) = &config.toolchain {
      cargo = cargo.arg(format!("+{toolchain}"));
    }
    cargo = cargo.args(["clippy", "--workspace"]);
    if config.all_targets {
      cargo = cargo.arg("--all-targets");
    }
    if config.all_features {
      cargo = cargo.arg("--all-features");
    } else if !config.features.is_empty() {
      cargo = cargo.arg("--features").arg(config.features.join(","));
    }
    cargo = cargo.args(&config.args);
    if config.deny_warnings {
      if !config.args.iter().any(|a| a == "--") {
        cargo = cargo.arg("--");
      }
      cargo = cargo.args(["-D", "warnings"]);
    }
    cargo
  }
}

impl Check for Clippy {
  fn name(&self) -> String {
    "clippy".to_owned()
  }

  fn run(&self, shell: &mut Shell, _: &CheckOptions) -> Result<Outcome> {
    let command = self.command();
    shell.status("Checking", "clippy")?;
    shell.verbose(|s| s.status("Running", &command))?;
    let status = command.status_unchecked()?;
    Ok(if status.success() {
      Outcome::passed("no warnings")
    } else {
      Outcome::failed(format!("cargo clippy failed ({status})"), Vec::new())
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn command_follows_the_config() {
    let clippy = Clippy::new("/repo", ClippyCheck::default());
    assert_eq!(
      clippy.command().to_string(),
      "cargo clippy --workspace --all-targets -- -D warnings"
    );
    let clippy = Clippy::new(
      "/repo",
      ClippyCheck {
        toolchain: Some("nightly".into()),
        all_targets: false,
        deny_warnings: false,
        features: vec!["a".into(), "b".into()],
        args: vec!["--locked".into()],
        ..ClippyCheck::default()
      },
    );
    assert_eq!(
      clippy.command().to_string(),
      "cargo +nightly clippy --workspace --features a,b --locked"
    );
  }
}
