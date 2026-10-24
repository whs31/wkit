use wkit_build::BuildOptions;
use wkit_package::{Distro, Installer, Packaging, Privileged};

use crate::context::Context;

#[derive(Debug, clap::Args)]
pub struct UninstallArgs {
  /// Deployment set to remove
  #[arg(long, value_name = "SET")]
  set: Option<String>,

  /// Only print the commands that need root
  #[arg(long)]
  dry_run: bool,
}

impl UninstallArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let project = ctx.project()?;
    let packaging = Packaging::new(&project, BuildOptions::default(), self.set, None)?;
    Installer::new(&packaging, Privileged::new(self.dry_run), Distro::detect())
      .uninstall(ctx.shell)?;
    if self.dry_run {
      ctx.shell.warn("nothing removed due to --dry-run")?;
    }
    Ok(())
  }
}
