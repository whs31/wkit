mod test_runner;
mod test_suite;

pub use self::{test_runner::TestRunner, test_suite::TestSuite};

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct TestSection {
  pub junit_dir: Option<std::path::PathBuf>,

  #[serde(rename = "suite")]
  pub suites: Vec<TestSuite>,
}

impl TestSection {
  pub const DEFAULT_JUNIT_DIR: &str = "build/junit";

  pub fn junit_dir(&self) -> &std::path::Path {
    self
      .junit_dir
      .as_deref()
      .unwrap_or(std::path::Path::new(Self::DEFAULT_JUNIT_DIR))
  }
}
