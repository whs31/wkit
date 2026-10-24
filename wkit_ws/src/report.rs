use wkit_core::{Shell, shell::Elapsed};

use crate::{Result, TaskOutcome, TaskStatus};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
  pub outcomes: Vec<(String, TaskOutcome)>,
}

impl Report {
  pub const TAIL: usize = 40;

  pub fn count(&self, status: TaskStatus) -> usize {
    self
      .outcomes
      .iter()
      .filter(|(_, o)| o.status == status)
      .count()
  }

  pub fn failures(&self) -> impl Iterator<Item = &(String, TaskOutcome)> {
    self.outcomes.iter().filter(|(_, o)| o.is_failed())
  }

  pub fn outcome(&self, path: &str) -> Option<&TaskOutcome> {
    self
      .outcomes
      .iter()
      .find(|(p, _)| p == path)
      .map(|(_, o)| o)
  }

  pub fn summary(&self, verb: &str) -> String {
    let mut parts = vec![format!("{} {verb}", self.count(TaskStatus::Done))];
    for (status, label) in [
      (TaskStatus::Warning, "with warnings"),
      (TaskStatus::Fresh, "unchanged"),
      (TaskStatus::Failed, "failed"),
    ] {
      let count = self.count(status);
      if count > 0 {
        parts.push(format!("{count} {label}"));
      }
    }
    parts.join(", ")
  }

  pub fn finish(&self, shell: &mut Shell, verb: &str, start: std::time::Instant) -> Result<()> {
    for (path, outcome) in self.failures() {
      let lines: Vec<_> = outcome.output.trim().lines().collect();
      if lines.is_empty() {
        continue;
      }
      shell.error(format_args!("{path}: {}", outcome.detail))?;
      let err = shell.err();
      for line in &lines[lines.len().saturating_sub(Self::TAIL)..] {
        writeln!(err, "  | {line}")?;
      }
    }
    let failed = self.count(TaskStatus::Failed);
    if failed > 0 {
      return Err(crate::Error::Message(format!(
        "{failed} of {} projects failed ({})",
        self.outcomes.len(),
        self.summary(verb)
      )));
    }
    shell.status(
      "Finished",
      format_args!("{} in {}", self.summary(verb), Elapsed(start.elapsed())),
    )?;
    Ok(())
  }
}
