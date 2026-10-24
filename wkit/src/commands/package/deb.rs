use wkit_build::BuildOptions;
use wkit_package::{CargoDeb, DebPackager};

use super::PackageSelection;
use crate::{commands::build_selection::BuildSelection, context::Context};

#[derive(Debug, clap::Args)]
#[command(mut_arg("build_type", |a| a.help("Build type [default: Release]")))]
pub struct DebArgs {
  #[command(flatten)]
  selection: BuildSelection,

  #[command(flatten)]
  package: PackageSelection,

  /// Number of parallel jobs
  #[arg(short, long, value_name = "N")]
  jobs: Option<std::num::NonZeroUsize>,

  /// Arguments passed to cargo-deb
  #[arg(last = true, value_name = "ARGS")]
  args: Vec<String>,
}

impl DebArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let project = ctx.project()?;
    let options = BuildOptions {
      jobs: self.jobs.map(std::num::NonZeroUsize::get),
      ..self.selection.options()
    };
    let packaging = self.package.packaging(&project, options)?;
    if packaging.is_cargo_project() {
      CargoDeb::new(&packaging).run(ctx.shell, &self.args)?;
    } else {
      if !self.args.is_empty() {
        anyhow::bail!("extra arguments only apply to cargo-deb");
      }
      DebPackager::new(&packaging).run(ctx.shell)?;
    }
    Ok(())
  }
}
