use wkit_manifest::{TestRunner, TestSuite};

use super::PlannedSuite;
use crate::SuiteContext;

impl PlannedSuite {
  pub(super) fn cargo(ctx: &SuiteContext<'_>, suite: &TestSuite, name: String) -> Self {
    let build = &ctx.project.manifest.build.cargo;
    let mut process = build.command(ctx.root(), &["test", "--workspace"]);
    if ctx.options.release {
      process = process.arg("--release");
    }
    process = process.args(&suite.args);
    if !ctx.options.extra.is_empty() {
      process = process.arg("--").args(&ctx.options.extra);
    }
    Self {
      name,
      runner: TestRunner::Cargo,
      process,
      junit: None,
      generated: Vec::new(),
    }
  }
}
