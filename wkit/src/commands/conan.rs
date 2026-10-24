mod sync;

use self::sync::SyncArgs;
use crate::context::Context;

#[derive(Debug, clap::Args)]
pub struct ConanArgs {
  #[command(subcommand)]
  command: ConanCommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum ConanCommand {
  /// Upload the resolved dependency graph from the local cache to a remote
  Sync(SyncArgs),
}

impl ConanArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    match self.command {
      ConanCommand::Sync(args) => args.run(ctx),
    }
  }
}
