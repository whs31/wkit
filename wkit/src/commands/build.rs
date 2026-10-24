use wkit_build::{BuildOptions, Builder};

use super::build_selection::BuildSelection;
use crate::context::Context;

#[derive(Debug, clap::Args)]
pub struct BuildArgs {
  #[command(flatten)]
  selection: BuildSelection,

  /// Also build the tests
  #[arg(short, long)]
  tests: bool,

  /// Use only packages that are already available locally
  #[arg(long)]
  offline: bool,

  /// Number of parallel jobs
  #[arg(short, long, value_name = "N")]
  jobs: Option<std::num::NonZeroUsize>,

  /// Arguments passed to the underlying build tool
  #[arg(last = true, value_name = "ARGS")]
  args: Vec<String>,
}

impl BuildArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let project = ctx.project()?;
    let options = BuildOptions {
      tests: self.tests,
      jobs: self.jobs.map(std::num::NonZeroUsize::get),
      offline: self.offline,
      extra: self.args,
      ..self.selection.options()
    };
    Builder::new(&project, &options).run(ctx.shell)?;
    Ok(())
  }
}
