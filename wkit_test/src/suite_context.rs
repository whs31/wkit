use wkit_manifest::{Project, TestSuite};

use crate::{BuildDir, Result, TestOptions};

#[derive(Debug, Clone, Copy)]
pub struct SuiteContext<'a> {
  pub project: &'a Project,
  pub options: &'a TestOptions,
  pub ci_seed: Option<&'a str>,
  pub headless: bool,
}

impl SuiteContext<'_> {
  pub fn root(&self) -> &std::path::Path {
    &self.project.root
  }

  pub fn build_dir(&self, suite: &TestSuite) -> Result<std::path::PathBuf> {
    BuildDir::resolve(
      self.project,
      suite.build_dir.as_deref(),
      self.options.build_type,
      self.options.build_dir.as_deref(),
    )
  }

  pub fn junit_file(&self, name: &str) -> Option<std::path::PathBuf> {
    self.options.junit.as_ref().map(|dir| {
      self
        .root()
        .join(dir)
        .join("suites")
        .join(format!("{name}.xml"))
    })
  }
}
