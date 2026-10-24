use wkit_build::BuildOptions;
use wkit_package::{Distro, Installer, Packaging, Privileged};

use super::build_selection::BuildSelection;
use crate::context::Context;

#[derive(Debug, clap::Args)]
#[command(mut_arg("build_type", |a| a.help("Build type [default: Release]")))]
pub struct InstallArgs {
  #[command(flatten)]
  selection: BuildSelection,

  /// Deployment set, passed to the Conan option in `package.set-option`
  #[arg(long, value_name = "SET")]
  set: Option<String>,

  /// Copy the `[install]` files even when the distro has a package format
  #[arg(long)]
  files: bool,

  /// Build, but only print the commands that need root
  #[arg(long)]
  dry_run: bool,

  /// Number of parallel jobs
  #[arg(short, long, value_name = "N")]
  jobs: Option<std::num::NonZeroUsize>,
}

impl InstallArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let project = ctx.project()?;
    let options = BuildOptions {
      jobs: self.jobs.map(std::num::NonZeroUsize::get),
      ..self.selection.options()
    };
    let packaging = Packaging::new(&project, options, self.set, None)?;
    Installer::new(&packaging, Privileged::new(self.dry_run), Distro::detect())
      .install(ctx.shell, self.files)?;
    if self.dry_run {
      ctx.shell.warn("nothing installed due to --dry-run")?;
    }
    Ok(())
  }
}
