use wkit_ws::{Committer, Workspace};

use super::ws_options::{Jobs, Publish};
use crate::context::Context;

#[derive(Debug, clap::Args)]
pub struct InitArgs {
  #[command(flatten)]
  jobs: Jobs,
}

#[derive(Debug, clap::Args)]
pub struct RestoreArgs {
  #[command(flatten)]
  jobs: Jobs,
}

#[derive(Debug, clap::Args)]
pub struct UpdateArgs {
  #[command(flatten)]
  jobs: Jobs,

  /// With `--commit`: relock and commit `workspace.lock`
  #[command(flatten)]
  publish: Publish,
}

#[derive(Debug, clap::Args)]
pub struct LockArgs {
  #[command(flatten)]
  jobs: Jobs,

  #[command(flatten)]
  publish: Publish,
}

impl InitArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    let workspace = Workspace::discover(&ctx.cwd)?;
    let locked = workspace.is_fully_locked();
    let report = workspace.init(ctx.shell, self.jobs.runner())?;
    report.finish(ctx.shell, "cloned", start)?;
    if !locked {
      workspace.relock(ctx.shell, self.jobs.jobs)?;
    }
    Ok(())
  }
}

impl RestoreArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    let workspace = Workspace::discover(&ctx.cwd)?;
    let report = workspace.restore(ctx.shell, self.jobs.runner())?;
    report.finish(ctx.shell, "restored", start)?;
    Ok(())
  }
}

impl UpdateArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    let workspace = Workspace::discover(&ctx.cwd)?;
    let report = workspace.update(ctx.shell, self.jobs.runner())?;
    report.finish(ctx.shell, "updated", start)?;
    if self.publish.commit {
      workspace.relock(ctx.shell, self.jobs.jobs)?;
      Committer::new(&workspace).commit_lock(ctx.shell, self.publish.push, self.jobs.jobs)?;
    }
    Ok(())
  }
}

impl LockArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let workspace = Workspace::discover(&ctx.cwd)?;
    workspace.relock(ctx.shell, self.jobs.jobs)?;
    if self.publish.commit {
      Committer::new(&workspace).commit_lock(ctx.shell, self.publish.push, self.jobs.jobs)?;
    }
    Ok(())
  }
}
