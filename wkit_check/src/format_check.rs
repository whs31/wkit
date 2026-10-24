use wkit_core::Shell;
use wkit_fmt::{Formatter, Mode};

use crate::{Check, CheckOptions, Outcome, Result};

#[derive(Debug, Clone)]
pub struct FormatCheck {
  formatter: Formatter,
}

impl FormatCheck {
  pub fn new(formatter: Formatter) -> Self {
    Self { formatter }
  }
}

impl Check for FormatCheck {
  fn name(&self) -> String {
    "format".to_owned()
  }

  fn run(&self, shell: &mut Shell, options: &CheckOptions) -> Result<Outcome> {
    let files = self.formatter.run(shell, Mode::Check, options.jobs)?;
    if files.is_empty() {
      return Ok(Outcome::passed("formatted"));
    }
    Ok(Outcome::failed(
      format!(
        "{} unformatted; run `wkit fmt`",
        Outcome::plural(files.len(), "file")
      ),
      files.iter().map(|f| format!("  {}", f.display())).collect(),
    ))
  }
}
