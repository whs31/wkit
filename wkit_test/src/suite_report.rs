use wkit_manifest::TestRunner;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuiteReport {
  pub name: String,
  pub runner: TestRunner,
  pub success: bool,
  pub code: Option<i32>,
  pub elapsed: std::time::Duration,
  pub junit: Option<String>,
}
