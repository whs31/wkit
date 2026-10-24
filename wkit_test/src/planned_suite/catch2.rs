use wkit_manifest::{TestRunner, TestSuite};

use super::PlannedSuite;
use crate::{Error, Result, SuiteContext};

impl PlannedSuite {
  pub(super) fn catch2(ctx: &SuiteContext<'_>, suite: &TestSuite, name: String) -> Result<Self> {
    let dir = ctx.build_dir(suite)?;
    let binary = suite.binary.as_deref().unwrap_or(&name);
    let path = wkit_core::fs::find_executable(&dir, binary).ok_or_else(|| {
      Error::setup(
        format!("test binary `{binary}` not found in `{}`", dir.display()),
        Some("check `binary` in `[[test.suite]]`".to_owned()),
      )
    })?;
    let mut process = wkit_core::ProcessBuilder::new(path).cwd(ctx.root());
    if let Some(filter) = &suite.filter {
      process = process.arg(filter);
    }
    if let Some(seed) = ctx.ci_seed {
      process = process.args(["--order", "rand", "--rng-seed", seed]);
    }
    let colour = if ctx.options.color { "ansi" } else { "none" };
    let junit = ctx.junit_file(&name);
    match &junit {
      Some(junit) => {
        process = process
          .arg("--reporter")
          .arg(format!("JUnit::out={}", junit.display()))
          .arg("--reporter")
          .arg(format!("console::out=-::colour-mode={colour}"));
      }
      None => process = process.args(["--colour-mode", colour]),
    }
    process = process.args(&suite.args).args(&ctx.options.extra);
    Ok(Self {
      name,
      runner: TestRunner::Catch2,
      process,
      junit,
      generated: Vec::new(),
    })
  }
}
