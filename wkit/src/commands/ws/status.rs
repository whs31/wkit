use wkit_ws::Workspace;

use super::ws_options::Jobs;
use crate::{context::Context, failure::Failure};

#[derive(Debug, clap::Args)]
pub struct StatusArgs {
  #[command(flatten)]
  jobs: Jobs,
}

impl StatusArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let workspace = Workspace::discover(&ctx.cwd)?;
    let statuses = workspace.status(self.jobs.jobs)?;
    let width = statuses
      .iter()
      .map(|s| s.branch.as_deref().unwrap_or("detached").len())
      .max()
      .unwrap_or_default();
    let out = ctx.shell.out();
    for status in &statuses {
      writeln!(out, "{}", status.render(width))?;
    }
    if statuses.iter().any(|s| s.broken) {
      return Err(Failure::silent(1).into());
    }
    Ok(())
  }
}
