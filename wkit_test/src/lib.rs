mod build_dir;
mod error;
mod junit_report;
mod planned_suite;
mod suite_context;
mod suite_report;
mod test_options;
mod tester;

pub use self::{
  build_dir::BuildDir, error::Error, junit_report::JunitReport, planned_suite::PlannedSuite,
  suite_context::SuiteContext, suite_report::SuiteReport, test_options::TestOptions,
  tester::Tester,
};

pub type Result<T, E = Error> = std::result::Result<T, E>;
