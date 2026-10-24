mod command;
mod custom_check;

use wkit_core::ProcessBuilder;

pub use self::{command::Command, custom_check::CustomCheck};
use crate::de::Shorthand;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(try_from = "Shorthand<TaskDetail>")]
pub struct Task {
  pub run: Command,
  pub description: Option<String>,
  pub cwd: Option<std::path::PathBuf>,
  pub env: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct TaskDetail {
  run: Command,
  description: Option<String>,
  cwd: Option<std::path::PathBuf>,

  #[serde(default)]
  env: std::collections::BTreeMap<String, String>,
}

impl Task {
  pub fn to_process(&self, root: &std::path::Path) -> ProcessBuilder {
    self.to_process_with(root, &[])
  }

  pub fn to_process_with(&self, root: &std::path::Path, extra: &[String]) -> ProcessBuilder {
    let cwd = root.join(self.cwd.as_deref().unwrap_or(std::path::Path::new(".")));
    self
      .env
      .iter()
      .fold(self.run.to_process_with(extra).cwd(cwd), |p, (k, v)| {
        p.env(k, v)
      })
  }
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
      shorthand => Ok(Self {
        run: Command::from_shorthand(shorthand)?,
        description: None,
        cwd: None,
        env: std::collections::BTreeMap::new(),
      }),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn detailed_task_runs_in_its_cwd_with_env() {
    let task: Task = toml::from_str::<std::collections::BTreeMap<String, Task>>(
      "t = { run = \"qmllint *.qml\", cwd = \"qml\", env = { QT_QPA_PLATFORM = \"offscreen\" } }",
    )
    .unwrap()
    .remove("t")
    .unwrap();
    let process = task.to_process(std::path::Path::new("/repo"));
    assert_eq!(process.get_cwd(), Some(std::path::Path::new("/repo/qml")));
    assert_eq!(
      process.get_env("QT_QPA_PLATFORM"),
      Some(std::ffi::OsStr::new("offscreen"))
    );
  }
}
