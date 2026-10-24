use wkit_manifest::{TestRunner, TestSuite};

use super::PlannedSuite;
use crate::{BuildDir, Result, SuiteContext};

impl PlannedSuite {
  pub(super) fn ctest(ctx: &SuiteContext<'_>, suite: &TestSuite, name: String) -> Result<Self> {
    let dir = BuildDir::resolve(ctx.project, suite.build_dir.as_deref(), ctx.build_type)?;
    let mut process = wkit_core::ProcessBuilder::new("ctest")
      .cwd(ctx.root())
      .arg("--test-dir")
      .arg(&dir)
      .args(["--output-on-failure", "--build-config"])
      .arg(ctx.build_type.to_string());
    if let Some(filter) = &suite.filter {
      process = process.args(["-R", filter]);
    }
    let junit = ctx.junit_file(&name);
    if let Some(junit) = &junit {
      process = process.arg("--output-junit").arg(junit);
    }
    process = process.args(&suite.args).args(&ctx.options.extra);
    Ok(Self {
      name,
      runner: TestRunner::Ctest,
      process,
      junit,
      generated: Vec::new(),
    })
  }
}
