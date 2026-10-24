use wkit_build::BuildOptions;
use wkit_package::WindowsPackager;

use super::PackageSelection;
use crate::{commands::build_selection::BuildSelection, context::Context};

#[derive(Debug, clap::Args)]
#[command(mut_arg("build_type", |a| a.help("Build type [default: Release]")))]
pub struct WindowsArgs {
  #[command(flatten)]
  selection: BuildSelection,

  #[command(flatten)]
  package: PackageSelection,

  /// Number of parallel jobs
  #[arg(short, long, value_name = "N")]
  jobs: Option<std::num::NonZeroUsize>,

  /// Arguments passed to cpack
  #[arg(last = true, value_name = "ARGS")]
  args: Vec<String>,
}

impl WindowsArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let project = ctx.project()?;
    let options = BuildOptions {
      jobs: self.jobs.map(std::num::NonZeroUsize::get),
      ..self.selection.options()
    };
    let packaging = self.package.packaging(&project, options)?;
    WindowsPackager::new(&packaging).run(ctx.shell, &self.args)?;
    Ok(())
  }
}
