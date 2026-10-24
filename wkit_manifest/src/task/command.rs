use wkit_core::ProcessBuilder;

use crate::de::Shorthand;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(try_from = "Shorthand<serde::de::IgnoredAny>")]
pub enum Command {
  Shell(String),
  Argv(Vec<String>),
}

impl Command {
  pub fn to_process(&self) -> ProcessBuilder {
    match self {
      Self::Shell(line) => ProcessBuilder::shell(line),
      Self::Argv(argv) => ProcessBuilder::new(&argv[0]).args(&argv[1..]),
    }
  }

  pub(crate) fn from_shorthand<T>(shorthand: Shorthand<T>) -> Result<Self, &'static str> {
    match shorthand {
      Shorthand::Str(line) if !line.trim().is_empty() => Ok(Self::Shell(line)),
      Shorthand::Seq(argv) if argv.first().is_some_and(|p| !p.is_empty()) => Ok(Self::Argv(argv)),
      Shorthand::Str(_) | Shorthand::Seq(_) => Err("command must not be empty"),
      Shorthand::Map(_) => Err("expected a shell command string or an array of arguments"),
    }
  }
}

impl TryFrom<Shorthand<serde::de::IgnoredAny>> for Command {
  type Error = &'static str;

  fn try_from(value: Shorthand<serde::de::IgnoredAny>) -> Result<Self, Self::Error> {
    Self::from_shorthand(value)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn command(toml: &str) -> Result<Command, String> {
    #[derive(serde::Deserialize)]
    struct Wrap {
      c: Command,
    }
    toml::from_str::<Wrap>(toml)
      .map(|w| w.c)
      .map_err(|e| e.message().to_owned())
  }

  #[test]
  fn strings_are_shell_lines_and_arrays_are_argv() {
    assert_eq!(
      command(r#"c = "make all""#).unwrap(),
      Command::Shell("make all".into())
    );
    assert_eq!(
      command(r#"c = ["cargo", "fmt"]"#)
        .unwrap()
        .to_process()
        .to_string(),
      "cargo fmt"
    );
  }

  #[test]
  fn empty_and_table_commands_are_rejected() {
    assert_eq!(
      command(r#"c = " ""#).unwrap_err(),
      "command must not be empty"
    );
    assert_eq!(command("c = []").unwrap_err(), "command must not be empty");
    assert!(
      command("c = { run = \"x\" }")
        .unwrap_err()
        .starts_with("expected a shell command")
    );
  }
}
