mod build;
mod build_selection;
mod bump;
mod check;
mod clean;
mod conan;
mod configure;
mod create;
mod external;
mod fmt;
mod graph;
mod init;
mod install;
mod package;
mod run;
mod run_script;
mod self_update;
mod test;
mod uninstall;

use self::{
  build::BuildArgs, bump::BumpArgs, check::CheckArgs, clean::CleanArgs, conan::ConanArgs,
  configure::ConfigureArgs, create::CreateArgs, external::External, fmt::FmtArgs, graph::GraphArgs,
  init::InitArgs, install::InstallArgs, package::PackageArgs, run::RunArgs,
  run_script::RunScriptArgs, self_update::SelfUpdateArgs, test::TestArgs, uninstall::UninstallArgs,
};
use crate::context::Context;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Show, bump or assign the project version in every version file
  Bump(BumpArgs),

  /// Format Rust and C/C++ sources
  #[command(alias = "format")]
  Fmt(FmtArgs),

  /// Build the project and its dependencies
  Build(BuildArgs),

  /// Install dependencies and configure the build without compiling
  Configure(ConfigureArgs),

  /// Build a `[[bin]]` and run it
  Run(RunArgs),

  /// Run format, version, lint and custom checks
  Check(CheckArgs),

  /// Run the project's test suites
  Test(TestArgs),

  /// Remove build outputs and generated files
  Clean(CleanArgs),

  /// Create the Conan package in the local cache and run its test package
  Create(CreateArgs),

  /// Render the Conan dependency graph as HTML
  Graph(GraphArgs),

  /// Conan helpers
  #[command(subcommand_value_name = "COMMAND")]
  Conan(ConanArgs),

  /// Build an Arch, Debian or Windows package
  #[command(subcommand_value_name = "FORMAT")]
  Package(PackageArgs),

  /// Build and install the project system-wide (apt, pacman or `[install]` files)
  Install(InstallArgs),

  /// Remove what `wkit install` installed
  Uninstall(UninstallArgs),

  /// Run a script from `[scripts]`
  #[command(visible_alias = "x")]
  RunScript(RunScriptArgs),

  /// Update wkit from crates.io
  SelfUpdate(SelfUpdateArgs),

  /// Create wkit.toml from what the project contains
  Init(InitArgs),

  #[command(external_subcommand)]
  External(Vec<String>),
}

impl Command {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    match self {
      Self::Bump(args) => args.run(ctx),
      Self::Fmt(args) => args.run(ctx),
      Self::Build(args) => args.run(ctx),
      Self::Configure(args) => args.run(ctx),
      Self::Run(args) => args.run(ctx),
      Self::Check(args) => args.run(ctx),
      Self::Test(args) => args.run(ctx),
      Self::Clean(args) => args.run(ctx),
      Self::Create(args) => args.run(ctx),
      Self::Graph(args) => args.run(ctx),
      Self::Conan(args) => args.run(ctx),
      Self::Package(args) => args.run(ctx),
      Self::Install(args) => args.run(ctx),
      Self::Uninstall(args) => args.run(ctx),
      Self::RunScript(args) => args.run(ctx),
      Self::SelfUpdate(args) => args.run(ctx),
      Self::Init(args) => args.run(ctx),
      Self::External(args) => External::new(args).run(ctx),
    }
  }
}
