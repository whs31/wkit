use crate::{Outcome, Status};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageReport {
  pub name: String,
  pub outcome: Outcome,
  pub elapsed: std::time::Duration,
}

impl StageReport {
  pub fn failed(&self) -> bool {
    self.outcome.status == Status::Failed
  }
}
