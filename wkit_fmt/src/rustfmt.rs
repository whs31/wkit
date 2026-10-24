use wkit_core::{ProcessBuilder, Result, Shell, detect::RustfmtChannel};
use wkit_manifest::Project;

use crate::Mode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rustfmt {
  root: std::path::PathBuf,
  channel: RustfmtChannel,
  args: Vec<String>,
}

impl Rustfmt {
  pub fn new(project: &Project) -> Result<Option<Self>> {
    let manifest = &project.manifest;
    if !manifest.rust_format_enabled() {
      return Ok(None);
    }
    let config = &manifest.format.rust;
    let channel = match config.channel {
      Some(channel) => channel,
      None => RustfmtChannel::from_path(&project.root)?,
    };
    Ok(Some(Self {
      root: project.root.clone(),
      channel,
      args: config.args.clone(),
    }))
  }

  pub fn channel(&self) -> RustfmtChannel {
    self.channel
  }

  pub fn command(&self, mode: Mode) -> ProcessBuilder {
    let mut cargo = ProcessBuilder::new("cargo").cwd(&self.root);
    if self.channel == RustfmtChannel::Nightly {
      cargo = cargo.arg("+nightly");
    }
    cargo = cargo.args(["fmt", "--all"]);
    if mode == Mode::Check {
      cargo = cargo.arg("--check");
    }
    cargo = cargo.args(&self.args);
    if !self.args.iter().any(|a| a == "--") {
      cargo = cargo.arg("--");
    }
    cargo.arg("-l")
  }

  pub fn run(&self, shell: &mut Shell, mode: Mode) -> Result<Vec<std::path::PathBuf>> {
    let command = self.command(mode);
    shell.status(
      mode.label(),
      format_args!("Rust sources ({})", self.label()),
    )?;
    shell.verbose(|s| s.status("Running", &command))?;
    let output = command.output_unchecked()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.trim().is_empty() {
      std::io::Write::write_all(shell.err(), stderr.as_bytes())?;
    }
    let files = self.listed_files(&String::from_utf8_lossy(&output.stdout));
    if output.status.success() || (mode == Mode::Check && !files.is_empty()) {
      Ok(files)
    } else {
      Err(wkit_core::Error::ProcessFailed {
        command: command.to_string(),
        status: output.status,
        stderr: String::new(),
      })
    }
  }

  fn label(&self) -> &'static str {
    match self.channel {
      RustfmtChannel::Stable => "cargo fmt",
      RustfmtChannel::Nightly => "cargo +nightly fmt",
    }
  }

  fn listed_files(&self, stdout: &str) -> Vec<std::path::PathBuf> {
    let mut files: Vec<_> = stdout
      .lines()
      .map(str::trim)
      .filter(|l| !l.is_empty())
      .map(|l| {
        let path = std::path::Path::new(l);
        path.strip_prefix(&self.root).unwrap_or(path).to_owned()
      })
      .collect();
    files.sort();
    files.dedup();
    files
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn rustfmt(channel: RustfmtChannel, args: &[&str]) -> Rustfmt {
    Rustfmt {
      root: "/repo".into(),
      channel,
      args: args.iter().map(|&a| a.to_owned()).collect(),
    }
  }

  #[test]
  fn command_follows_channel_mode_and_args() {
    assert_eq!(
      rustfmt(RustfmtChannel::Stable, &[])
        .command(Mode::Write)
        .to_string(),
      "cargo fmt --all -- -l"
    );
    assert_eq!(
      rustfmt(RustfmtChannel::Nightly, &["--", "--edition", "2024"])
        .command(Mode::Check)
        .to_string(),
      "cargo +nightly fmt --all --check -- --edition 2024 -l"
    );
  }

  #[test]
  fn listed_files_are_relative_and_unique() {
    let files = rustfmt(RustfmtChannel::Stable, &[])
      .listed_files("/repo/src/b.rs\n/repo/src/a.rs\n\n/repo/src/a.rs\n/elsewhere/c.rs\n");
    assert_eq!(
      files,
      [
        std::path::Path::new("/elsewhere/c.rs"),
        std::path::Path::new("src/a.rs"),
        std::path::Path::new("src/b.rs"),
      ]
    );
  }
}
