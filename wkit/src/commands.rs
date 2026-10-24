mod build;
mod build_selection;
mod bump;
mod check;
mod clean;
mod conan;
mod configure;
mod create;
mod doctor;
mod external;
mod fmt;
mod graph;
mod info;
mod init;
mod install;
mod package;
mod run;
mod run_script;
mod self_update;
mod smoke;
mod test;
mod uninstall;
mod ws;

use self::{
  build::BuildArgs, bump::BumpArgs, check::CheckArgs, clean::CleanArgs, conan::ConanArgs,
  configure::ConfigureArgs, create::CreateArgs, doctor::DoctorArgs, external::External,
  fmt::FmtArgs, graph::GraphArgs, info::InfoArgs, init::InitArgs, install::InstallArgs,
  package::PackageArgs, run::RunArgs, run_script::RunScriptArgs, self_update::SelfUpdateArgs,
  smoke::SmokeArgs, test::TestArgs, uninstall::UninstallArgs, ws::WsArgs,
};
use crate::context::Context;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Show, bump or assign the project version in every version file
  Bump(BumpArgs),

  /// Format Rust, C/C++, QML, Python and Lua sources
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

  /// Launch a `[[bin]]` repeatedly to catch crashes on startup
  Smoke(SmokeArgs),

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

  /// Manage the repositories of a multi-repo workspace (`workspace.toml`)
  #[command(subcommand_value_name = "COMMAND")]
  Ws(WsArgs),

  /// Run a script from `[scripts]`
  #[command(visible_alias = "x")]
  RunScript(RunScriptArgs),

  /// Check that the tools this project needs are installed
  Doctor(DoctorArgs),

  /// Show the host and a summary of the project
  Info(InfoArgs),

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
      Self::Smoke(args) => args.run(ctx),
      Self::Clean(args) => args.run(ctx),
      Self::Create(args) => args.run(ctx),
      Self::Graph(args) => args.run(ctx),
      Self::Conan(args) => args.run(ctx),
      Self::Package(args) => args.run(ctx),
      Self::Install(args) => args.run(ctx),
      Self::Uninstall(args) => args.run(ctx),
      Self::Ws(args) => args.run(ctx),
      Self::RunScript(args) => args.run(ctx),
      Self::Doctor(_) => DoctorArgs::run(ctx),
      Self::Info(args) => args.run(ctx),
      Self::SelfUpdate(args) => args.run(ctx),
      Self::Init(args) => args.run(ctx),
      Self::External(args) => External::new(args).run(ctx),
    }
  }
}
