use wkit_manifest::{BuildType, Project};

use crate::TestOptions;

#[derive(Debug, Clone, Copy)]
pub struct SuiteContext<'a> {
  pub project: &'a Project,
  pub options: &'a TestOptions,
  pub build_type: BuildType,
  pub ci_seed: Option<&'a str>,
  pub headless: bool,
}

impl SuiteContext<'_> {
  pub fn root(&self) -> &std::path::Path {
    &self.project.root
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
