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
    self.to_process_with(&[])
  }

  pub fn to_process_with(&self, extra: &[String]) -> ProcessBuilder {
    match self {
      Self::Shell(_) => ProcessBuilder::shell(&self.line_with(extra)),
      Self::Argv(argv) => ProcessBuilder::new(&argv[0]).args(&argv[1..]).args(extra),
    }
  }

  pub fn line_with(&self, extra: &[String]) -> String {
    let (head, tail): (String, &[String]) = match self {
      Self::Shell(line) => (line.clone(), &[]),
      Self::Argv(argv) => (
        wkit_core::process::ShellQuoted(&argv[0]).to_string(),
        &argv[1..],
      ),
    };
    tail.iter().chain(extra).fold(head, |line, arg| {
      format!("{line} {}", wkit_core::process::ShellQuoted(arg))
    })
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
  fn lines_show_what_runs() {
    let extra = ["a b".to_owned()];
    assert_eq!(
      Command::Shell("make all".into()).line_with(&extra),
      "make all 'a b'"
    );
    assert_eq!(
      Command::Argv(vec!["my tool".into(), "-x".into()]).line_with(&extra),
      "'my tool' -x 'a b'"
    );
  }

  #[test]
  fn extra_args_are_appended() {
    let extra = ["--check".to_owned(), "a b".to_owned()];
    assert_eq!(
      Command::Argv(vec!["cargo".into(), "fmt".into()])
        .to_process_with(&extra)
        .get_args(),
      ["fmt", "--check", "a b"]
    );
    let shell = Command::Shell("make all".into()).to_process_with(&extra);
    assert!(
      shell
        .get_args()
        .last()
        .is_some_and(|line| line == "make all --check 'a b'")
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
