use wkit_build::BuildOptions;
use wkit_package::{ArchPackager, Privileged};

use super::PackageSelection;
use crate::context::Context;

#[derive(Debug, clap::Args)]
pub struct ArchArgs {
  #[command(flatten)]
  package: PackageSelection,

  /// Arguments passed to makepkg
  #[arg(last = true, value_name = "ARGS")]
  args: Vec<String>,
}

impl ArchArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let project = ctx.project()?;
    let packaging = self.package.packaging(&project, BuildOptions::default())?;
    ArchPackager::new(&packaging)?.run(ctx.shell, Privileged::new(false), &self.args)?;
    Ok(())
  }
}
