use crate::{GitRun, TaskStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskOutcome {
  pub status: TaskStatus,
  pub detail: String,
  pub output: String,
}

impl TaskOutcome {
  pub fn done(detail: impl Into<String>) -> Self {
    Self::new(TaskStatus::Done, detail, String::new())
  }

  pub fn fresh(detail: impl Into<String>) -> Self {
    Self::new(TaskStatus::Fresh, detail, String::new())
  }

  pub fn warning(detail: impl Into<String>) -> Self {
    Self::new(TaskStatus::Warning, detail, String::new())
  }

  pub fn failed(detail: impl Into<String>) -> Self {
    Self::new(TaskStatus::Failed, detail, String::new())
  }

  pub fn git_failed(detail: impl Into<String>, run: &GitRun) -> Self {
    Self::new(TaskStatus::Failed, detail, run.output.clone())
  }

  #[must_use]
  pub fn with_warning(self, warning: Option<String>) -> Self {
    match warning {
      Some(warning) if self.status != TaskStatus::Failed => Self {
        status: TaskStatus::Warning,
        detail: format!("{}; {warning}", self.detail),
        ..self
      },
      _ => self,
    }
  }

  pub fn is_failed(&self) -> bool {
    self.status == TaskStatus::Failed
  }

  fn new(status: TaskStatus, detail: impl Into<String>, output: String) -> Self {
    Self {
      status,
      detail: detail.into(),
      output,
    }
  }
}
