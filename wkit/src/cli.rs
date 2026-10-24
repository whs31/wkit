use wkit_core::{
  Shell,
  shell::{ColorChoice, Verbosity, style},
};

use crate::{commands::Command, context::Context};

#[derive(Debug, clap::Parser)]
#[command(
  name = "wkit",
  version,
  about = "Version, format and clean C, C++ and Rust projects described by wkit.toml",
  styles = Cli::styles(),
  max_term_width = 100,
)]
pub struct Cli {
  /// Use verbose output
  #[arg(short, long, action = clap::ArgAction::Count, global = true)]
  verbose: u8,

  /// Print nothing but errors
  #[arg(short, long, global = true, conflicts_with = "verbose")]
  quiet: bool,

  /// Coloring
  #[arg(
    long,
    value_name = "WHEN",
    global = true,
    env = "WKIT_COLOR",
    default_value = "auto",
    value_parser = Cli::color_parser(),
  )]
  color: ColorChoice,

  /// Path to wkit.toml
  #[arg(long, value_name = "PATH", global = true, env = "WKIT_MANIFEST_PATH")]
  manifest_path: Option<std::path::PathBuf>,

  /// Change to DIR before doing anything
  #[arg(short = 'C', value_name = "DIR")]
  directory: Option<std::path::PathBuf>,

  #[command(subcommand)]
  command: Command,
}

impl Cli {
  pub fn shell(&self) -> Shell {
    let verbosity = match (self.quiet, self.verbose) {
      (true, _) => Verbosity::Quiet,
      (false, 0) => Verbosity::Normal,
      (false, _) => Verbosity::Verbose,
    };
    Shell::with(verbosity, self.color)
  }

  pub fn run(self, shell: &mut Shell) -> anyhow::Result<()> {
    if let Some(dir) = &self.directory {
      std::env::set_current_dir(dir)
        .map_err(|e| anyhow::anyhow!(e).context(format!("cannot change to `{}`", dir.display())))?;
    }
    let mut ctx = Context {
      shell,
      cwd: std::env::current_dir()?,
      manifest_path: self.manifest_path,
    };
    self.command.run(&mut ctx)
  }

  fn color_parser() -> impl clap::builder::TypedValueParser<Value = ColorChoice> {
    clap::builder::TypedValueParser::map(
      clap::builder::PossibleValuesParser::new(ColorChoice::ALL.map(|c| c.to_string())),
      |s| s.parse().expect("listed choices parse"),
    )
  }

  fn styles() -> clap::builder::Styles {
    clap::builder::Styles::styled()
      .header(style::HEADER)
      .usage(style::HEADER)
      .literal(style::PROGRESS)
      .placeholder(style::NOTE.bold().dimmed())
      .error(style::ERROR)
      .valid(style::PROGRESS)
      .invalid(style::WARN)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn cli_is_consistent() {
    <Cli as clap::CommandFactory>::command().debug_assert();
  }
}
