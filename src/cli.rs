use std::path::PathBuf;

use anyhow::Context as _;

use crate::{
  Result, Shell,
  commands::{Command, Run as _},
  manifest::{MANIFEST_FILE, Project},
  shell::{ColorChoice, Verbosity, style},
};

#[derive(Debug, clap::Parser)]
#[command(
  name = "wkit",
  version,
  about = "Version, format and clean C, C++ and Rust projects described by wkit.toml",
  styles = styles(),
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
    default_value = "auto"
  )]
  color: ColorChoice,

  /// Path to wkit.toml
  #[arg(long, value_name = "PATH", global = true, env = "WKIT_MANIFEST_PATH")]
  manifest_path: Option<PathBuf>,

  /// Change to DIR before doing anything
  #[arg(short = 'C', value_name = "DIR")]
  directory: Option<PathBuf>,

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
    Shell::new(verbosity, self.color)
  }

  pub fn run(self, shell: &mut Shell) -> Result<()> {
    if let Some(dir) = &self.directory {
      std::env::set_current_dir(dir)
        .with_context(|| format!("cannot change to `{}`", dir.display()))?;
    }
    let mut ctx = Context {
      shell,
      cwd: std::env::current_dir()?,
      manifest_path: self.manifest_path,
    };
    self.command.run(&mut ctx)
  }
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

pub struct Context<'a> {
  pub shell: &'a mut Shell,
  pub cwd: PathBuf,
  pub manifest_path: Option<PathBuf>,
}

impl Context<'_> {
  pub fn version() -> semver::Version {
    semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("package version is semver")
  }

  pub fn project(&mut self) -> Result<Project> {
    let project = self.try_project()?.ok_or_else(|| {
      anyhow::anyhow!(
        "could not find `{MANIFEST_FILE}` in `{}` or any parent directory",
        self.cwd.display()
      )
    })?;
    project.ensure_compatible(&Self::version())?;
    Ok(project)
  }

  pub fn try_project(&mut self) -> Result<Option<Project>> {
    let project = match &self.manifest_path {
      Some(path) => Some(Project::load(&self.cwd.join(path))?),
      None => Project::discover(&self.cwd)?,
    };
    for warning in project.iter().flat_map(|p| &p.manifest.warnings) {
      self.shell.warn(warning)?;
    }
    Ok(project)
  }
}

#[cfg(test)]
mod tests {
  #[test]
  fn cli_is_consistent() {
    <super::Cli as clap::CommandFactory>::command().debug_assert();
  }
}
