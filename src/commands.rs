macro_rules! subcommands {
  ($(#[$meta:meta])* $name:ident { $($(#[$attr:meta])* $variant:ident($args:ty),)* }) => {
    $(#[$meta])*
    #[derive(Debug, clap::Subcommand)]
    pub enum $name {
      $($(#[$attr])* $variant($args),)*
    }

    impl $crate::commands::Run for $name {
      fn run(self, ctx: &mut $crate::cli::Context<'_>) -> $crate::Result<()> {
        match self {
          $(Self::$variant(args) => args.run(ctx),)*
        }
      }
    }
  };
}

mod build;
mod init;
mod package;
mod project;
mod run;
mod tooling;
mod ws;

use crate::{Result, cli::Context};

pub trait Run {
  fn run(self, ctx: &mut Context<'_>) -> Result<()>;
}

impl Run for Vec<String> {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    project::external(ctx, self)
  }
}

subcommands! {
  Command {
    /// Show, bump or assign the project version in every version file
    Bump(project::BumpArgs),

    /// Format Rust, C/C++, QML, Python and Lua sources
    #[command(alias = "format")]
    Fmt(project::FmtArgs),

    /// Build the project and its dependencies
    Build(build::BuildArgs),

    /// Install dependencies and configure the build without compiling
    Configure(build::ConfigureArgs),

    /// Build a `[[bin]]` and run it
    Run(run::RunArgs),

    /// Run format, version, lint and custom checks
    Check(project::CheckArgs),

    /// Run the project's test suites
    Test(build::TestArgs),

    /// Launch a `[[bin]]` repeatedly to catch crashes on startup
    Smoke(run::SmokeArgs),

    /// Remove build outputs and generated files
    Clean(project::CleanArgs),

    /// Create the Conan package in the local cache and run its test package
    Create(build::CreateArgs),

    /// Render the Conan dependency graph as HTML
    Graph(build::GraphArgs),

    /// Conan helpers
    #[command(subcommand, subcommand_value_name = "COMMAND")]
    Conan(build::ConanCommand),

    /// Build an Arch, Debian or Windows package
    #[command(subcommand, subcommand_value_name = "FORMAT")]
    Package(package::PackageFormat),

    /// Build and install the project system-wide (apt, pacman or `[install]` files)
    Install(package::InstallArgs),

    /// Remove what `wkit install` installed
    Uninstall(package::UninstallArgs),

    /// Manage the repositories of a multi-repo workspace (`workspace.toml`)
    #[command(subcommand, subcommand_value_name = "COMMAND")]
    Ws(ws::WsCommand),

    /// Run a script from `[scripts]`
    #[command(visible_alias = "x")]
    RunScript(project::RunScriptArgs),

    /// Check that the tools this project needs are installed
    Doctor(tooling::DoctorArgs),

    /// Show the host and a summary of the project
    Info(tooling::InfoArgs),

    /// Update wkit from crates.io
    SelfUpdate(tooling::SelfUpdateArgs),

    /// Create wkit.toml from what the project contains
    Init(init::InitArgs),

    #[command(external_subcommand)]
    External(Vec<String>),
  }
}

fn jobs(jobs: Option<std::num::NonZeroUsize>) -> Option<usize> {
  jobs.map(std::num::NonZeroUsize::get)
}

fn exit_status(status: std::process::ExitStatus) -> Result<()> {
  if status.success() {
    Ok(())
  } else {
    Err(crate::Failure::silent(status.code().unwrap_or(crate::Failure::INTERNAL)).into())
  }
}
