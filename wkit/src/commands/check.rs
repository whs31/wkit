use wkit_check::{CheckOptions, Checker};
use wkit_core::shell::Verbosity;

use crate::{context::Context, failure::Failure};

#[derive(Debug, clap::Args)]
pub struct CheckArgs {
  /// Run only these stages (format, versions, clippy, clang-tidy, secrets, whitespace, pre-commit or a custom check)
  #[arg(long, value_name = "STAGE", value_delimiter = ',')]
  only: Vec<String>,

  /// Skip these stages
  #[arg(long, value_name = "STAGE", value_delimiter = ',')]
  skip: Vec<String>,

  /// Number of parallel clang-format and clang-tidy jobs
  #[arg(short, long, value_name = "N")]
  jobs: Option<std::num::NonZeroUsize>,

  /// Ignore cached clang-tidy results
  #[arg(long)]
  no_cache: bool,

  /// Build directory holding `compile_commands.json`
  #[arg(value_name = "BUILD_DIR")]
  build_dir: Option<std::path::PathBuf>,
}

impl CheckArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let project = ctx.project()?;
    let checker = Checker::plan(&project, &self.only, &self.skip)?;
    if checker.names().is_empty() {
      ctx.shell.warn("no checks to run")?;
      return Ok(());
    }
    let defaults = CheckOptions::default();
    let options = CheckOptions {
      jobs: self.jobs.map_or(defaults.jobs, std::num::NonZeroUsize::get),
      cache: !self.no_cache,
      build_dir: self.build_dir.map(|dir| ctx.cwd.join(dir)),
      verbose: ctx.shell.verbosity() == Verbosity::Verbose,
    };
    let start = std::time::Instant::now();
    let reports = checker.run(ctx.shell, &options)?;
    let failed: Vec<_> = reports
      .iter()
      .filter(|r| r.failed())
      .map(|r| r.name.as_str())
      .collect();
    if failed.is_empty() {
      ctx.shell.finished("checks", start.elapsed())?;
      return Ok(());
    }
    Err(Failure::new(format!("checks failed: {}", failed.join(", ")), 1).into())
  }
}
