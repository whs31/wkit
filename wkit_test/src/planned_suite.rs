mod cargo;
mod catch2;
mod ctest;
mod nextest;

use wkit_core::{ProcessBuilder, Shell};
use wkit_manifest::{TestRunner, TestSuite};

use crate::{Error, JunitReport, Result, SuiteContext, SuiteReport};

#[derive(Debug, Clone)]
pub struct PlannedSuite {
  pub name: String,
  pub runner: TestRunner,
  pub process: ProcessBuilder,
  pub junit: Option<std::path::PathBuf>,
  pub generated: Vec<(std::path::PathBuf, String)>,
}

impl PlannedSuite {
  pub const HEADLESS_ENV: (&str, &str) = ("QT_QPA_PLATFORM", "offscreen");

  pub fn plan(ctx: &SuiteContext<'_>, suite: &TestSuite) -> Result<Self> {
    let name = suite.display_name();
    let mut planned = match suite.runner {
      TestRunner::Cargo if ctx.options.junit.is_some() => {
        if which::which("cargo-nextest").is_err() {
          return Err(Error::setup(
            format!("test suite `{name}` runs `cargo test`, which cannot write JUnit reports"),
            Some("install cargo-nextest (`cargo install cargo-nextest --locked`) or use `runner = \"nextest\"`".to_owned()),
          ));
        }
        Self::nextest(ctx, suite, name)
      }
      TestRunner::Cargo => Self::cargo(ctx, suite, name),
      TestRunner::Nextest => Self::nextest(ctx, suite, name),
      TestRunner::Ctest => Self::ctest(ctx, suite, name)?,
      TestRunner::Catch2 => Self::catch2(ctx, suite, name)?,
    };
    for (key, value) in &suite.env {
      planned.process = planned.process.env(key, value);
    }
    let (key, value) = Self::HEADLESS_ENV;
    if ctx.headless && !suite.env.contains_key(key) && std::env::var_os(key).is_none() {
      planned.process = planned.process.env(key, value);
    }
    Ok(planned)
  }

  pub fn inferred(project: &wkit_manifest::Project) -> Vec<TestSuite> {
    let manifest = &project.manifest;
    let mut suites = Vec::new();
    if manifest.has(wkit_core::Language::Rust) {
      let runner = if which::which("cargo-nextest").is_ok() {
        TestRunner::Nextest
      } else {
        TestRunner::Cargo
      };
      suites.push(Self::bare(runner));
    }
    if manifest.has_c_family() {
      suites.push(Self::bare(TestRunner::Ctest));
    }
    suites
  }

  pub fn run(&self, shell: &mut Shell) -> Result<SuiteReport> {
    shell.status("Testing", format_args!("{} ({})", self.name, self.runner))?;
    shell.verbose(|s| s.status("Running", &self.process))?;
    for (path, contents) in &self.generated {
      Self::write(path, Some(contents))?;
    }
    if let Some(junit) = &self.junit {
      Self::write(junit, None)?;
    }
    let start = std::time::Instant::now();
    let status = self.process.status_unchecked()?;
    let elapsed = start.elapsed();
    let junit = self.junit.as_ref().map(|path| {
      std::fs::read_to_string(path).unwrap_or_else(|_| {
        JunitReport::failure(&self.name, &format!("no JUnit report ({status})"))
      })
    });
    Ok(SuiteReport {
      name: self.name.clone(),
      runner: self.runner,
      success: status.success(),
      code: status.code(),
      elapsed,
      junit,
    })
  }

  fn write(path: &std::path::Path, contents: Option<&str>) -> Result<()> {
    let _ = std::fs::remove_file(path);
    path
      .parent()
      .map_or(Ok(()), std::fs::create_dir_all)
      .and_then(|()| contents.map_or(Ok(()), |c| std::fs::write(path, c)))
      .map_err(|source| {
        wkit_core::Error::Write {
          path: path.to_owned(),
          source,
        }
        .into()
      })
  }

  fn bare(runner: TestRunner) -> TestSuite {
    TestSuite {
      name: None,
      runner,
      profile: None,
      binary: None,
      build_dir: None,
      filter: None,
      args: Vec::new(),
      env: std::collections::BTreeMap::new(),
    }
  }
}
