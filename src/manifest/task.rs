use std::collections::BTreeMap;

use crate::{ProcessBuilder, process::quote};

#[derive(Debug)]
pub enum Shorthand<T> {
  Str(String),
  Seq(Vec<String>),
  Map(T),
}

impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Shorthand<T> {
  fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    struct Visitor<T>(std::marker::PhantomData<T>);

    impl<'de, T: serde::Deserialize<'de>> serde::de::Visitor<'de> for Visitor<T> {
      type Value = Shorthand<T>;

      fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a string, an array of strings or a table")
      }

      fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(Shorthand::Str(v.to_owned()))
      }

      fn visit_seq<A: serde::de::SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
        serde::Deserialize::deserialize(serde::de::value::SeqAccessDeserializer::new(seq))
          .map(Shorthand::Seq)
      }

      fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        T::deserialize(serde::de::value::MapAccessDeserializer::new(map)).map(Shorthand::Map)
      }
    }

    deserializer.deserialize_any(Visitor(std::marker::PhantomData))
  }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(try_from = "Shorthand<TaskDetail>")]
pub struct Task {
  pub run: Command,
  pub description: Option<String>,
  pub cwd: Option<std::path::PathBuf>,
  pub env: BTreeMap<String, String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TaskDetail {
  run: Command,
  description: Option<String>,
  cwd: Option<std::path::PathBuf>,
  #[serde(default)]
  env: BTreeMap<String, String>,
}

impl Task {
  pub fn to_process(&self, root: &std::path::Path, extra: &[String]) -> ProcessBuilder {
    let cwd = root.join(self.cwd.as_deref().unwrap_or(std::path::Path::new(".")));
    self
      .env
      .iter()
      .fold(self.run.to_process(extra).cwd(cwd), |p, (k, v)| p.env(k, v))
  }
}

impl TryFrom<Shorthand<TaskDetail>> for Task {
  type Error = &'static str;

  fn try_from(value: Shorthand<TaskDetail>) -> Result<Self, Self::Error> {
    Ok(match value {
      Shorthand::Map(TaskDetail {
        run,
        description,
        cwd,
        env,
      }) => Self {
        run,
        description,
        cwd,
        env,
      },
      shorthand => Self {
        run: Command::from_shorthand(shorthand)?,
        description: None,
        cwd: None,
        env: BTreeMap::new(),
      },
    })
  }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(try_from = "Shorthand<serde::de::IgnoredAny>")]
pub enum Command {
  Shell(String),
  Argv(Vec<String>),
}

impl Command {
  pub fn to_process(&self, extra: &[String]) -> ProcessBuilder {
    match self {
      Self::Shell(_) => ProcessBuilder::shell(&self.line(extra)),
      Self::Argv(argv) => ProcessBuilder::new(&argv[0]).args(&argv[1..]).args(extra),
    }
  }

  pub fn line(&self, extra: &[String]) -> String {
    let (head, tail) = match self {
      Self::Shell(line) => (line.clone(), &[][..]),
      Self::Argv(argv) => (quote(&argv[0]), &argv[1..]),
    };
    tail
      .iter()
      .chain(extra)
      .fold(head, |line, arg| format!("{line} {}", quote(arg)))
  }

  fn from_shorthand<T>(shorthand: Shorthand<T>) -> Result<Self, &'static str> {
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
  fn detailed_task_runs_in_its_cwd_with_env() {
    let task: Task = toml::from_str::<BTreeMap<String, Task>>(
      "t = { run = \"qmllint *.qml\", cwd = \"qml\", env = { QT_QPA_PLATFORM = \"offscreen\" } }",
    )
    .unwrap()
    .remove("t")
    .unwrap();
    let process = task.to_process(std::path::Path::new("/repo"), &[]);
    assert_eq!(process.get_cwd(), Some(std::path::Path::new("/repo/qml")));
    assert_eq!(
      process.get_env("QT_QPA_PLATFORM"),
      Some(std::ffi::OsStr::new("offscreen"))
    );
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
        .to_process(&[])
        .to_string(),
      "cargo fmt"
    );
  }

  #[test]
  fn lines_show_what_runs() {
    let extra = ["a b".to_owned()];
    assert_eq!(
      Command::Shell("make all".into()).line(&extra),
      "make all 'a b'"
    );
    assert_eq!(
      Command::Argv(vec!["my tool".into(), "-x".into()]).line(&extra),
      "'my tool' -x 'a b'"
    );
  }

  #[test]
  fn extra_args_are_appended() {
    let extra = ["--check".to_owned(), "a b".to_owned()];
    assert_eq!(
      Command::Argv(vec!["cargo".into(), "fmt".into()])
        .to_process(&extra)
        .get_args(),
      ["fmt", "--check", "a b"]
    );
    let shell = Command::Shell("make all".into()).to_process(&extra);
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
