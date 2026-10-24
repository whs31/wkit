#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestOptions {
  pub junit: Option<std::path::PathBuf>,
  pub suites: Vec<String>,
  pub release: bool,
  pub extra: Vec<String>,
  pub color: bool,
}
