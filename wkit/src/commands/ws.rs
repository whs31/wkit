mod add;
mod commit;
mod exec;
mod fmt;
mod status;
mod sync;
mod ws_options;

use self::{
  add::AddArgs,
  commit::CommitArgs,
  exec::ExecArgs,
  fmt::FmtArgs,
  status::StatusArgs,
  sync::{InitArgs, LockArgs, RestoreArgs, UpdateArgs},
};
use crate::context::Context;

#[derive(Debug, clap::Args)]
pub struct WsArgs {
  #[command(subcommand)]
  command: WsCommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum WsCommand {
  /// Clone missing projects at the locked snapshot
  Init(InitArgs),

  /// Return clean projects to the locked snapshot
  Restore(RestoreArgs),

  /// Fast-forward projects along their development branches
  Update(UpdateArgs),

  /// Record every clean project's HEAD in `workspace.lock`
  Lock(LockArgs),

  /// Show branch, worktree, upstream and lock state
  Status(StatusArgs),

  /// Clone a new project and register it
  Add(AddArgs),

  /// Commit pending project formatting or `workspace.lock`
  #[command(subcommand_value_name = "WHAT")]
  Commit(CommitArgs),

  /// Run a command in every cloned project
  Exec(ExecArgs),

  /// Format every project (`wkit fmt`, else its `just fmt`)
  Fmt(FmtArgs),
}

impl WsArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    match self.command {
      WsCommand::Init(args) => args.run(ctx),
      WsCommand::Restore(args) => args.run(ctx),
      WsCommand::Update(args) => args.run(ctx),
      WsCommand::Lock(args) => args.run(ctx),
      WsCommand::Status(args) => args.run(ctx),
      WsCommand::Add(args) => args.run(ctx),
      WsCommand::Commit(args) => args.run(ctx),
      WsCommand::Exec(args) => args.run(ctx),
      WsCommand::Fmt(args) => args.run(ctx),
    }
  }
}
