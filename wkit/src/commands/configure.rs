use wkit_build::{BuildOptions, Builder};

use super::build_selection::BuildSelection;
use crate::context::Context;

#[derive(Debug, clap::Args)]
pub struct ConfigureArgs {
  #[command(flatten)]
  selection: BuildSelection,

  /// Configure with the tests enabled
  #[arg(short, long)]
  tests: bool,

  /// Arguments passed to `conan install`, or to `cmake` without Conan
  #[arg(last = true, value_name = "ARGS")]
  args: Vec<String>,
}

impl ConfigureArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let project = ctx.project()?;
    let options = BuildOptions {
      tests: self.tests,
      extra: self.args,
      ..self.selection.options()
    };
    if Builder::new(&project, &options)
      .configure(ctx.shell)?
      .is_none()
    {
      ctx
        .shell
        .warn("nothing to configure: the project has no Conan or CMake build")?;
    }
    Ok(())
  }
}
