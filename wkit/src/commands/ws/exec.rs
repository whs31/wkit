use wkit_core::ProcessBuilder;
use wkit_ws::Workspace;

use crate::{context::Context, failure::Failure};

#[derive(Debug, clap::Args)]
pub struct ExecArgs {
  /// Stop at the first project where the command fails
  #[arg(long)]
  fail_fast: bool,

  /// A shell command as one argument, or a program and its arguments
  #[arg(
    required = true,
    trailing_var_arg = true,
    allow_hyphen_values = true,
    value_name = "COMMAND"
  )]
  command: Vec<String>,
}

impl ExecArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    let workspace = Workspace::discover(&ctx.cwd)?;
    let members = workspace.cloned();
    if members.is_empty() {
      ctx.shell.warn("no cloned projects (run `wkit ws init`)")?;
      return Ok(());
    }
    let shown = self.command.join(" ");
    let mut failed = Vec::new();
    for member in &members {
      ctx
        .shell
        .status("Running", format_args!("`{shown}` in {}", member.path))?;
      let process = match self.command.as_slice() {
        [line] => ProcessBuilder::shell(line),
        [program, args @ ..] => ProcessBuilder::new(program).args(args),
        [] => unreachable!("clap requires a command"),
      };
      let status = process
        .cwd(member.dir(&workspace.root))
        .status_unchecked()?;
      if !status.success() {
        ctx.shell.error(format_args!("{}: {status}", member.path))?;
        failed.push(member.path.as_str());
        if self.fail_fast {
          break;
        }
      }
    }
    if !failed.is_empty() {
      return Err(
        Failure::new(
          format!(
            "{} of {} projects failed: {}",
            failed.len(),
            members.len(),
            failed.join(", ")
          ),
          1,
        )
        .into(),
      );
    }
    ctx
      .shell
      .finished(format_args!("{} projects", members.len()), start.elapsed())?;
    Ok(())
  }
}
