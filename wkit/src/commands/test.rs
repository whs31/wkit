use wkit_build::{BuildOptions, Builder};
use wkit_core::shell::ColorChoice;
use wkit_test::{TestOptions, Tester};

use super::build_selection::BuildSelection;
use crate::{context::Context, failure::Failure};

#[derive(Debug, clap::Args)]
pub struct TestArgs {
  /// Write merged test reports to `DIR/junit.xml` [default: `test.junit-dir`]
  #[allow(clippy::option_option)]
  #[arg(long, value_name = "DIR", num_args = 0..=1)]
  junit: Option<Option<std::path::PathBuf>>,

  /// Run only this suite (repeatable)
  #[arg(long = "suite", value_name = "NAME")]
  suites: Vec<String>,

  #[command(flatten)]
  selection: BuildSelection,

  /// Arguments passed to every test runner
  #[arg(last = true, value_name = "ARGS")]
  args: Vec<String>,
}

impl TestArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let project = ctx.project()?;
    let junit = self.junit.map(|dir| match dir {
      Some(dir) => ctx.cwd.join(dir),
      None => project.join(project.manifest.test.junit_dir()),
    });
    let suites = Tester::select(&project, &self.suites)?;
    let build_options = BuildOptions {
      tests: true,
      ..self.selection.options()
    };
    let builder = Builder::new(&project, &build_options);
    let built = if suites.iter().any(|s| s.runner.needs_build_dir()) {
      builder.run_native(ctx.shell)?
    } else {
      None
    };
    let options = TestOptions {
      junit,
      suites: self.suites,
      build_type: match &built {
        Some(outcome) => outcome.build_type,
        None => builder.build_type()?,
      },
      build_dir: built
        .as_ref()
        .and_then(|outcome| outcome.build_dir().map(ToOwned::to_owned)),
      extra: self.args,
      color: match ctx.shell.color_choice() {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => std::io::IsTerminal::is_terminal(&std::io::stdout()),
      },
    };
    let tester = Tester::plan(&project, &options)?;
    if tester.suites().is_empty() {
      ctx.shell.warn("no test suites to run")?;
      return Ok(());
    }
    let start = std::time::Instant::now();
    let reports = tester.run(ctx.shell)?;
    let failed: Vec<_> = reports.iter().filter(|r| !r.success).collect();
    let Some(first) = failed.first() else {
      ctx.shell.finished("tests", start.elapsed())?;
      return Ok(());
    };
    let names: Vec<_> = failed.iter().map(|r| r.name.as_str()).collect();
    Err(
      Failure::new(
        format!("test suites failed: {}", names.join(", ")),
        first.code.filter(|&c| c != 0).unwrap_or(1),
      )
      .into(),
    )
  }
}
