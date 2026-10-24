mod crate_index;
mod index_entry;

use wkit_core::ProcessBuilder;

use self::crate_index::CrateIndex;
use crate::{context::Context, failure::Failure};

#[derive(Debug, clap::Args)]
pub struct SelfUpdateArgs {
  /// Install this exact version instead of the newest one
  #[arg(long, value_name = "VERSION")]
  version: Option<semver::Version>,
}

impl SelfUpdateArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let current = Context::version();
    let requirement = match ctx.try_project() {
      Ok(project) => project.and_then(|p| p.manifest.project.wkit),
      Err(e) => {
        ctx
          .shell
          .warn(format_args!("ignoring `project.wkit`: {e}"))?;
        None
      }
    };
    if let (Some(pinned), Some(requirement)) = (&self.version, &requirement)
      && !requirement.matches(pinned)
    {
      ctx.shell.warn(format_args!(
        "wkit {pinned} does not satisfy `project.wkit = \"{requirement}\"`"
      ))?;
    }
    ctx.shell.status("Checking", "crates.io for wkit updates")?;
    let target =
      CrateIndex::fetch()?.select(&current, requirement.as_ref(), self.version.as_ref())?;
    if self.version.is_none() && target <= current {
      ctx
        .shell
        .status("Fresh", format_args!("wkit v{current} is up to date"))?;
      return Ok(());
    }
    if target == current {
      ctx.shell.status(
        "Fresh",
        format_args!("wkit v{current} is already installed"),
      )?;
      return Ok(());
    }
    ctx
      .shell
      .status("Updating", format_args!("wkit v{current} -> v{target}"))?;
    let install = Self::installer(&target);
    ctx.shell.verbose(|s| s.status("Running", &install))?;
    let parked = Self::park_running_exe();
    let status = install.status_unchecked()?;
    if !status.success() {
      if let Some((exe, old)) = parked {
        let _ = std::fs::rename(old, exe);
      }
      return Err(
        Failure::new(
          format!("`{install}` failed ({status})"),
          status.code().unwrap_or(Failure::INTERNAL),
        )
        .into(),
      );
    }
    ctx
      .shell
      .status("Updated", format_args!("wkit to v{target}"))?;
    Ok(())
  }

  fn installer(target: &semver::Version) -> ProcessBuilder {
    if which::which("cargo-binstall").is_ok() {
      ProcessBuilder::new("cargo")
        .args(["binstall", "--no-confirm", "--force"])
        .arg(format!("wkit@={target}"))
    } else {
      ProcessBuilder::new("cargo")
        .args(["install", "wkit", "--locked", "--force", "--version"])
        .arg(format!("={target}"))
    }
  }

  fn park_running_exe() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    if !cfg!(windows) {
      return None;
    }
    let exe = std::env::current_exe().ok()?;
    let old = exe.with_extension("exe.old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).ok()?;
    Some((exe, old))
  }
}
