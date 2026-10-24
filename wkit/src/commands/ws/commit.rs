use wkit_ws::{Committer, Member, Runner, Workspace};

use crate::context::Context;

#[derive(Debug, clap::Args)]
pub struct CommitArgs {
  #[command(subcommand)]
  what: CommitWhat,

  /// Number of projects to process in parallel
  #[arg(short, long, global = true, value_name = "N", default_value_t = Runner::DEFAULT_JOBS)]
  jobs: usize,

  /// Push the new commits
  #[arg(long, global = true)]
  push: bool,
}

#[derive(Debug, Clone, Copy, clap::Subcommand)]
pub enum CommitWhat {
  /// Commit every project's uncommitted tracked changes as `CHORE: Formatting`
  Fmt,

  /// Commit `workspace.lock` as `CHORE: Update lock`, listing what moved
  Lock,
}

impl CommitArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let workspace = Workspace::discover(&ctx.cwd)?;
    let committer = Committer::new(&workspace);
    match self.what {
      CommitWhat::Lock => committer.commit_lock(ctx.shell, self.push, self.jobs)?,
      CommitWhat::Fmt => {
        let start = std::time::Instant::now();
        let changed = Workspace::parallel(&workspace.manifest.members, self.jobs, |m| {
          let repo = workspace.repo(m);
          repo.is_standalone() && !repo.changed_files().is_empty()
        });
        let members: Vec<Member> = workspace
          .manifest
          .members
          .iter()
          .zip(changed)
          .filter(|(_, changed)| *changed)
          .map(|(m, _)| m.clone())
          .collect();
        if members.is_empty() {
          ctx
            .shell
            .status("Fresh", "no formatting changes to commit")?;
          return Ok(());
        }
        let push = self.push;
        let report = Runner::new(self.jobs).run(
          ctx.shell,
          "Committing",
          "Committed",
          &members,
          |member, phase| committer.commit_member(member, Committer::FORMATTING, push, phase),
        )?;
        report.finish(ctx.shell, if push { "pushed" } else { "committed" }, start)?;
      }
    }
    Ok(())
  }
}
