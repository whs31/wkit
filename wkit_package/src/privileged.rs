use wkit_core::{ProcessBuilder, Shell};

use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Privileged {
  sudo: bool,
  dry_run: bool,
}

impl Privileged {
  pub fn new(dry_run: bool) -> Self {
    Self {
      sudo: !Self::is_root(),
      dry_run,
    }
  }

  pub fn is_dry_run(&self) -> bool {
    self.dry_run
  }

  pub fn command(&self, program: &str) -> ProcessBuilder {
    if self.sudo {
      ProcessBuilder::new("sudo").arg(program)
    } else {
      ProcessBuilder::new(program)
    }
  }

  pub fn run(&self, shell: &mut Shell, process: &ProcessBuilder) -> Result<()> {
    if self.dry_run {
      shell.status("Would run", process)?;
      return Ok(());
    }
    shell.verbose(|s| s.status("Running", process))?;
    process.status()?;
    Ok(())
  }

  fn is_root() -> bool {
    ProcessBuilder::new("id")
      .arg("-u")
      .output()
      .is_ok_and(|output| output.stdout.trim_ascii() == b"0")
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn sudo_wraps_commands_for_regular_users() {
    let privileged = Privileged {
      sudo: true,
      dry_run: true,
    };
    assert_eq!(
      privileged.command("install").arg("-d").to_string(),
      "sudo install -d"
    );
    let root = Privileged {
      sudo: false,
      ..privileged
    };
    assert_eq!(root.command("install").to_string(), "install");
  }
}
