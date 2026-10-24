use wkit_ws::{Committer, Workspace, WorkspaceLock, WorkspaceManifest};

use super::ws_options::Publish;
use crate::context::Context;

#[derive(Debug, clap::Args)]
pub struct AddArgs {
  /// Git remote URL
  url: String,

  /// Workspace path [default: derived from the URL and the existing layout]
  path: Option<String>,

  /// Development branch [default: the remote's HEAD branch]
  #[arg(short, long, value_name = "BRANCH")]
  branch: Option<String>,

  #[command(flatten)]
  publish: Publish,
}

impl AddArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let mut workspace = Workspace::discover(&ctx.cwd)?;
    let member = workspace.add(
      ctx.shell,
      &self.url,
      self.path.as_deref(),
      self.branch.as_deref(),
    )?;
    if self.publish.commit {
      Committer::new(&workspace).commit_root(
        ctx.shell,
        &format!("FEAT: Add {}", member.name()),
        &[
          WorkspaceManifest::FILE,
          WorkspaceLock::FILE,
          Workspace::GITIGNORE,
        ],
        self.publish.push,
      )?;
    }
    Ok(())
  }
}
