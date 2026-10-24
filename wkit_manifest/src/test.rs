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
