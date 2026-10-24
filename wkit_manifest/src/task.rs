use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, de::IgnoredAny};
use wkit_core::ProcessBuilder;

use crate::de::Shorthand;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "Shorthand<IgnoredAny>")]
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

  fn new(shorthand: Shorthand<impl Sized>) -> Result<Self, &'static str> {
    match shorthand {
      Shorthand::Str(line) if !line.trim().is_empty() => Ok(Self::Shell(line)),
      Shorthand::Seq(argv) if argv.first().is_some_and(|p| !p.is_empty()) => Ok(Self::Argv(argv)),
      Shorthand::Str(_) | Shorthand::Seq(_) => Err("command must not be empty"),
      Shorthand::Map(_) => Err("expected a shell command string or an array of arguments"),
    }
  }
}

impl TryFrom<Shorthand<IgnoredAny>> for Command {
  type Error = &'static str;

  fn try_from(value: Shorthand<IgnoredAny>) -> Result<Self, Self::Error> {
    Self::new(value)
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "Shorthand<TaskDetail>")]
pub struct Task {
  pub run: Command,
  pub description: Option<String>,
  pub cwd: Option<PathBuf>,
  pub env: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct TaskDetail {
  run: Command,
  description: Option<String>,
  cwd: Option<PathBuf>,
  #[serde(default)]
  env: BTreeMap<String, String>,
}

impl TryFrom<Shorthand<TaskDetail>> for Task {
  type Error = &'static str;

  fn try_from(value: Shorthand<TaskDetail>) -> Result<Self, Self::Error> {
    match value {
      Shorthand::Map(detail) => Ok(Self {
        run: detail.run,
        description: detail.description,
        cwd: detail.cwd,
        env: detail.env,
      }),
      other => Ok(Self {
        run: Command::new(other)?,
        description: None,
        cwd: None,
        env: BTreeMap::new(),
      }),
    }
  }
}

impl Task {
  pub fn to_process(&self, root: &std::path::Path) -> ProcessBuilder {
    let process = self
      .run
      .to_process()
      .cwd(root.join(self.cwd.as_deref().unwrap_or(".".as_ref())));
    self.env.iter().fold(process, |p, (k, v)| p.env(k, v))
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CustomCheck {
  pub name: String,
  pub run: Command,
  pub description: Option<String>,
  pub cwd: Option<PathBuf>,
  #[serde(default)]
  pub env: BTreeMap<String, String>,
}

impl CustomCheck {
  pub fn task(&self) -> Task {
    Task {
      run: self.run.clone(),
      description: self.description.clone(),
      cwd: self.cwd.clone(),
      env: self.env.clone(),
    }
  }
}
