use wkit_ws::{Committer, FmtTarget, Member, TaskOutcome, Workspace};

use super::ws_options::{Jobs, Publish};
use crate::context::Context;

#[derive(Debug, clap::Args)]
pub struct FmtArgs {
  #[command(flatten)]
  jobs: Jobs,

  /// With `--commit`: commit the changes as `CHORE: Formatting` in each project
  #[command(flatten)]
  publish: Publish,
}

impl FmtArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    let workspace = Workspace::discover(&ctx.cwd)?;
    let jobs = self.jobs.jobs;
    let targets = Workspace::parallel(&workspace.manifest.members, jobs, |member| {
      FmtTarget::detect(&member.dir(&workspace.root))
    });
    let (members, targets): (Vec<Member>, Vec<FmtTarget>) = workspace
      .manifest
      .members
      .iter()
      .cloned()
      .zip(targets)
      .filter_map(|(member, target)| target.map(|t| (member, t)))
      .unzip();
    if members.is_empty() {
      ctx
        .shell
        .warn("no project has a `wkit.toml` or a `fmt`/`format` just recipe (are they cloned?)")?;
      return Ok(());
    }
    let dirty = if self.publish.commit {
      Workspace::parallel(&members, jobs, |m| {
        !workspace.repo(m).changed_files().is_empty()
      })
    } else {
      vec![false; members.len()]
    };
    let wkit = std::env::current_exe()?;
    let report = self.jobs.runner().run(
      ctx.shell,
      "Formatting",
      "Formatted",
      &members,
      |member, phase| {
        let index = members
          .iter()
          .position(|m| m == member)
          .expect("member is listed");
        phase.set(&targets[index].label(), None);
        Self::format(&targets[index], &member.dir(&workspace.root), &wkit)
      },
    )?;
    report.finish(ctx.shell, "formatted", start)?;
    if self.publish.commit {
      self.commit(ctx, &workspace, &members, &dirty, start)?;
    }
    Ok(())
  }

  fn format(target: &FmtTarget, dir: &std::path::Path, wkit: &std::path::Path) -> TaskOutcome {
    match target.command(dir, wkit).output_unchecked() {
      Ok(output) if output.status.success() => {
        TaskOutcome::done(format!("via `{}`", target.label()))
      }
      Ok(output) => TaskOutcome {
        output: format!(
          "{}{}",
          String::from_utf8_lossy(&output.stdout),
          String::from_utf8_lossy(&output.stderr)
        ),
        ..TaskOutcome::failed(format!("`{}` failed ({})", target.label(), output.status))
      },
      Err(e) => TaskOutcome::failed(e.to_string()),
    }
  }

  fn commit(
    &self,
    ctx: &mut Context<'_>,
    workspace: &Workspace,
    members: &[Member],
    dirty: &[bool],
    start: std::time::Instant,
  ) -> anyhow::Result<()> {
    for (member, _) in members.iter().zip(dirty).filter(|(_, dirty)| **dirty) {
      ctx.shell.warn(format_args!(
        "{} had uncommitted changes before formatting, not committing it",
        member.path
      ))?;
    }
    let clean: Vec<Member> = members
      .iter()
      .zip(dirty)
      .filter(|(m, dirty)| !**dirty && !workspace.repo(m).changed_files().is_empty())
      .map(|(m, _)| m.clone())
      .collect();
    if clean.is_empty() {
      ctx
        .shell
        .status("Fresh", "no formatting changes to commit")?;
      return Ok(());
    }
    let committer = Committer::new(workspace);
    let push = self.publish.push;
    let report = self.jobs.runner().run(
      ctx.shell,
      "Committing",
      "Committed",
      &clean,
      |member, phase| committer.commit_member(member, Committer::FORMATTING, push, phase),
    )?;
    report.finish(ctx.shell, if push { "pushed" } else { "committed" }, start)?;
    Ok(())
  }
}
