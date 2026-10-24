use wkit_build::{BuildOptions, Builder};

use super::build_selection::BuildSelection;
use crate::context::Context;

#[derive(Debug, clap::Args)]
pub struct CreateArgs {
  #[command(flatten)]
  selection: BuildSelection,

  /// Number of parallel jobs
  #[arg(short, long, value_name = "N")]
  jobs: Option<std::num::NonZeroUsize>,

  /// Arguments passed to `conan create`
  #[arg(last = true, value_name = "ARGS")]
  args: Vec<String>,
}

impl CreateArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let project = ctx.project()?;
    let options = BuildOptions {
      jobs: self.jobs.map(std::num::NonZeroUsize::get),
      extra: self.args,
      ..self.selection.options()
    };
    Builder::new(&project, &options).create(ctx.shell)?;
    Ok(())
  }
}
