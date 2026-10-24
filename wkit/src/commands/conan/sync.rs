use wkit_build::{Builder, RemoteSync};

use crate::{commands::build_selection::BuildSelection, context::Context};

#[derive(Debug, clap::Args)]
pub struct SyncArgs {
  #[command(flatten)]
  selection: BuildSelection,

  /// Remote to upload to [default: `conan.sync.remote`]
  #[arg(long, value_name = "NAME")]
  remote: Option<String>,

  /// Check what would be uploaded without uploading
  #[arg(long)]
  dry_run: bool,
}

impl SyncArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let project = ctx.project()?;
    let options = self.selection.options();
    let build_type = Builder::new(&project, &options).build_type()?;
    let sync = RemoteSync::new(&project, &options, self.remote, self.dry_run);
    let count = sync.run(ctx.shell, build_type)?;
    let remote = sync.remote().unwrap_or_default();
    let what = format!(
      "{count} dependenc{} with `{remote}`",
      if count == 1 { "y" } else { "ies" }
    );
    if self.dry_run {
      ctx.shell.status("Checked", what)?;
      ctx.shell.warn("nothing uploaded due to --dry-run")?;
    } else {
      ctx.shell.status("Synchronized", what)?;
    }
    Ok(())
  }
}
