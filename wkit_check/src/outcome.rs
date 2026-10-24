use crate::Status;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
  pub status: Status,
  pub summary: String,
  pub details: Vec<String>,
}

impl Outcome {
  pub fn passed(summary: impl Into<String>) -> Self {
    Self {
      status: Status::Passed,
      summary: summary.into(),
      details: Vec::new(),
    }
  }

  pub fn failed(summary: impl Into<String>, details: Vec<String>) -> Self {
    Self {
      status: Status::Failed,
      summary: summary.into(),
      details,
    }
  }

  pub fn skipped(summary: impl Into<String>) -> Self {
    Self {
      status: Status::Skipped,
      summary: summary.into(),
      details: Vec::new(),
    }
  }

  pub fn plural(count: usize, word: &str) -> String {
    if count == 1 {
      format!("1 {word}")
    } else {
      format!("{count} {word}s")
    }
  }
}
