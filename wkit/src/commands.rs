mod build;
mod build_selection;
mod bump;
mod check;
mod clean;
mod external;
mod fmt;
mod init;
mod run_script;
mod self_update;
mod test;

use self::{
  build::BuildArgs, bump::BumpArgs, check::CheckArgs, clean::CleanArgs, external::External,
  fmt::FmtArgs, init::InitArgs, run_script::RunScriptArgs, self_update::SelfUpdateArgs,
  test::TestArgs,
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

  /// Run format, version, lint and custom checks
  Check(CheckArgs),

  /// Run the project's test suites
  Test(TestArgs),

  /// Remove build outputs and generated files
  Clean(CleanArgs),

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
      Self::Check(args) => args.run(ctx),
      Self::Test(args) => args.run(ctx),
      Self::Clean(args) => args.run(ctx),
      Self::RunScript(args) => args.run(ctx),
      Self::SelfUpdate(args) => args.run(ctx),
      Self::Init(args) => args.run(ctx),
      Self::External(args) => External::new(args).run(ctx),
    }
  }
}
