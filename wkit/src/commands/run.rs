use super::build_selection::BuildSelection;
use crate::{context::Context, failure::Failure, launcher::Launcher};

#[derive(Debug, clap::Args)]
pub struct RunArgs {
  /// The `[[bin]]` or executable to run [default: `project.default-run`]
  #[arg(long, value_name = "NAME")]
  bin: Option<String>,

  #[command(flatten)]
  selection: BuildSelection,

  /// Arguments passed to the program
  #[arg(last = true, value_name = "ARGS")]
  args: Vec<String>,
}

impl RunArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let project = ctx.project()?;
    let options = self.selection.clone().options();
    let launcher = Launcher::new(&project, &options, self.bin.as_deref())?;
    let mut process = launcher.build(ctx.shell, true)?;
    if process.get_cwd().is_none() {
      process = process.cwd(&ctx.cwd);
    }
    if process.get_program() != "cargo" {
      ctx
        .shell
        .status("Running", format_args!("`{}`", launcher.display(&process)))?;
    }
    let status = process.args(&self.args).status_unchecked()?;
    if status.success() {
      Ok(())
    } else {
      Err(Failure::silent(status.code().unwrap_or(1)).into())
    }
  }
}
