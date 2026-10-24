mod build;
mod check;
mod cli;
mod commands;
mod detect;
mod format;
mod fs;
mod manifest;
mod package;
mod process;
mod shell;
mod testing;
mod tool;
mod version;
mod ws;

pub use self::{detect::Language, process::ProcessBuilder, shell::Shell};

pub type Result<T, E = anyhow::Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
#[error("{}", message.as_deref().unwrap_or_default())]
pub struct Failure {
  pub message: Option<String>,
  pub code: i32,
}

impl Failure {
  pub const INTERNAL: i32 = 101;

  pub fn new(message: impl Into<String>, code: i32) -> Self {
    Self {
      message: Some(message.into()),
      code,
    }
  }

  pub fn silent(code: i32) -> Self {
    Self {
      message: None,
      code,
    }
  }

  pub fn exit_code(error: &anyhow::Error) -> i32 {
    error
      .chain()
      .find_map(|cause| cause.downcast_ref::<Self>())
      .map_or(Self::INTERNAL, |f| f.code)
  }
}

pub fn hint(message: impl std::fmt::Display, help: impl std::fmt::Display) -> anyhow::Error {
  anyhow::anyhow!("{message}\n\nhelp: {help}")
}

fn main() {
  let cli = <cli::Cli as clap::Parser>::parse();
  let mut shell = cli.shell();
  if let Err(error) = cli.run(&mut shell) {
    std::process::exit(report(&mut shell, &error));
  }
}

fn report(shell: &mut Shell, error: &anyhow::Error) -> i32 {
  if let Some(Failure {
    message: None,
    code,
  }) = error.downcast_ref::<Failure>()
  {
    return *code;
  }
  let _ = shell.error(error);
  let mut causes = error.chain().skip(1).peekable();
  if causes.peek().is_some() {
    let err = shell.err();
    let _ = writeln!(err, "\nCaused by:");
    for cause in causes {
      for line in cause.to_string().lines() {
        let _ = writeln!(err, "  {line}");
      }
    }
  }
  Failure::exit_code(error)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn exit_codes_come_from_failures() {
    assert_eq!(Failure::exit_code(&Failure::silent(3).into()), 3);
    assert_eq!(
      Failure::exit_code(&anyhow::Error::from(Failure::new("x", 1)).context("outer")),
      1
    );
    assert_eq!(
      Failure::exit_code(&anyhow::anyhow!("internal")),
      Failure::INTERNAL
    );
  }
}
