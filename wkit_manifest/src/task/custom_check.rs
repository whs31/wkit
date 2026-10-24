use super::{Command, Task};

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CustomCheck {
  pub name: String,
  pub run: Command,
  pub description: Option<String>,
  pub cwd: Option<std::path::PathBuf>,

  #[serde(default)]
  pub env: std::collections::BTreeMap<String, String>,
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
