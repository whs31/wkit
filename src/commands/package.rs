use std::{num::NonZeroUsize, path::PathBuf};

use super::{Run, build::BuildSelection, jobs};
use crate::{
  Result, Shell,
  build::BuildOptions,
  cli::Context,
  package::{self, Packaging, Privileged},
};

#[derive(Debug, clap::Subcommand)]
pub enum PackageFormat {
  /// Build an Arch Linux package with makepkg from a git source tarball
  Arch(ArchArgs),

  /// Build a Debian package (cargo-deb for cargo projects)
  Deb(BuildPackageArgs),

  /// Build a Windows installer (NSIS, WIX or ZIP) through `cpack`
  Windows(BuildPackageArgs),
}

impl Run for PackageFormat {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    match self {
      Self::Arch(args) => {
        let project = ctx.project()?;
        let packaging = args.package.packaging(&project, BuildOptions::default())?;
        package::arch(&packaging, ctx.shell, Privileged::new(false), &args.args).map(drop)
      }
      Self::Deb(args) => args.build(ctx, |p, shell, extra| {
        package::deb(p, shell, extra).map(drop)
      }),
      Self::Windows(args) => args.build(ctx, |p, shell, extra| {
        package::windows(p, shell, extra).map(drop)
      }),
    }
  }
}

#[derive(Debug, Clone, Default, clap::Args)]
struct PackageSelection {
  /// Deployment set, passed to the Conan option in `package.set-option`
  #[arg(long, value_name = "SET")]
  set: Option<String>,

  /// Output directory [default: `package.output`, then build/package]
  #[arg(short, long, value_name = "DIR")]
  output: Option<PathBuf>,
}

impl PackageSelection {
  fn packaging(
    self,
    project: &crate::manifest::Project,
    options: BuildOptions,
  ) -> Result<Packaging<'_>> {
    Packaging::new(project, options, self.set, self.output)
  }
}

#[derive(Debug, clap::Args)]
pub struct ArchArgs {
  #[command(flatten)]
  package: PackageSelection,

  /// Arguments passed to makepkg
  #[arg(last = true, value_name = "ARGS")]
  args: Vec<String>,
}

#[derive(Debug, clap::Args)]
#[command(mut_arg("build_type", |a| a.help("Build type [default: Release]")))]
pub struct BuildPackageArgs {
  #[command(flatten)]
  selection: BuildSelection,

  #[command(flatten)]
  package: PackageSelection,

  /// Number of parallel jobs
  #[arg(short, long, value_name = "N")]
  jobs: Option<NonZeroUsize>,

  /// Arguments passed to cargo-deb or cpack
  #[arg(last = true, value_name = "ARGS")]
  args: Vec<String>,
}

impl BuildPackageArgs {
  fn build(
    self,
    ctx: &mut Context<'_>,
    package: impl FnOnce(&Packaging<'_>, &mut Shell, &[String]) -> Result<()>,
  ) -> Result<()> {
    let project = ctx.project()?;
    let options = BuildOptions {
      jobs: jobs(self.jobs),
      ..self.selection.options()
    };
    package(
      &self.package.packaging(&project, options)?,
      ctx.shell,
      &self.args,
    )
  }
}

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
  jobs: Option<NonZeroUsize>,
}

impl Run for InstallArgs {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    let project = ctx.project()?;
    let options = BuildOptions {
      jobs: jobs(self.jobs),
      ..self.selection.options()
    };
    let packaging = Packaging::new(&project, options, self.set, None)?;
    package::install(&packaging, ctx.shell, self.dry_run, self.files)?;
    if self.dry_run {
      ctx.shell.warn("nothing installed due to --dry-run")?;
    }
    Ok(())
  }
}

#[derive(Debug, clap::Args)]
pub struct UninstallArgs {
  /// Deployment set to remove
  #[arg(long, value_name = "SET")]
  set: Option<String>,

  /// Only print the commands that need root
  #[arg(long)]
  dry_run: bool,
}

impl Run for UninstallArgs {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    let project = ctx.project()?;
    let packaging = Packaging::new(&project, BuildOptions::default(), self.set, None)?;
    package::uninstall(&packaging, ctx.shell, self.dry_run)?;
    if self.dry_run {
      ctx.shell.warn("nothing removed due to --dry-run")?;
    }
    Ok(())
  }
}
