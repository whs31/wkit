use wkit_core::Shell;
use wkit_manifest::CustomCheck;

use crate::{Check, CheckOptions, Outcome, Result};

#[derive(Debug, Clone)]
pub struct Custom {
  root: std::path::PathBuf,
  check: CustomCheck,
}

impl Custom {
  pub fn new(root: impl Into<std::path::PathBuf>, check: CustomCheck) -> Self {
    Self {
      root: root.into(),
      check,
    }
  }
}

impl Check for Custom {
  fn name(&self) -> String {
    self.check.name.clone()
  }

  fn run(&self, shell: &mut Shell, _: &CheckOptions) -> Result<Outcome> {
    let task = self.check.task();
    shell.status(
      "Checking",
      self
        .check
        .description
        .as_deref()
        .unwrap_or(&self.check.name),
    )?;
    shell.verbose(|s| s.status("Running", format_args!("`{}`", task.run.line_with(&[]))))?;
    let status = task.to_process(&self.root).status_unchecked()?;
    Ok(if status.success() {
      Outcome::passed("passed")
    } else {
      Outcome::failed(format!("failed ({status})"), Vec::new())
    })
  }
}
