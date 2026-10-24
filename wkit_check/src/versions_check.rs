use wkit_core::Shell;
use wkit_version::VersionSet;

use crate::{Check, CheckOptions, Outcome, Result};

#[derive(Debug, Clone)]
pub struct VersionsCheck {
  root: std::path::PathBuf,
  section: wkit_manifest::VersionSection,
}

impl VersionsCheck {
  pub fn new(root: impl Into<std::path::PathBuf>, section: wkit_manifest::VersionSection) -> Self {
    Self {
      root: root.into(),
      section,
    }
  }
}

impl Check for VersionsCheck {
  fn name(&self) -> String {
    "versions".to_owned()
  }

  fn run(&self, shell: &mut Shell, _: &CheckOptions) -> Result<Outcome> {
    shell.status(
      "Checking",
      format_args!(
        "versions ({})",
        Outcome::plural(self.section.files.len(), "file")
      ),
    )?;
    let set = VersionSet::load(&self.root, &self.section)?;
    let current = set.current()?;
    let mismatches = set.mismatches(&current);
    if mismatches.is_empty() {
      return Ok(Outcome::passed(format!("all at {current}")));
    }
    Ok(Outcome::failed(
      format!(
        "{} disagree with {current}",
        Outcome::plural(mismatches.len(), "file")
      ),
      mismatches.iter().map(|m| format!("  {m}")).collect(),
    ))
  }
}
