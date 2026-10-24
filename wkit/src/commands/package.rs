mod arch;
mod deb;
mod package_selection;
mod windows;

pub use self::package_selection::PackageSelection;
use self::{arch::ArchArgs, deb::DebArgs, windows::WindowsArgs};
use crate::context::Context;

#[derive(Debug, clap::Args)]
pub struct PackageArgs {
  #[command(subcommand)]
  format: PackageFormat,
}

#[derive(Debug, clap::Subcommand)]
pub enum PackageFormat {
  /// Build an Arch Linux package with makepkg from a git source tarball
  Arch(ArchArgs),

  /// Build a Debian package (cargo-deb for cargo projects)
  Deb(DebArgs),

  /// Build a Windows installer (NSIS, WIX or ZIP) through `cpack`
  Windows(WindowsArgs),
}

impl PackageArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    match self.format {
      PackageFormat::Arch(args) => args.run(ctx),
      PackageFormat::Deb(args) => args.run(ctx),
      PackageFormat::Windows(args) => args.run(ctx),
    }
  }
}
