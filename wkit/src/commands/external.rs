use wkit_core::ProcessBuilder;

use super::run_script::RunScriptArgs;
use crate::{context::Context, failure::Failure};

#[derive(Debug, Clone)]
pub struct External {
  name: String,
  args: Vec<String>,
}

impl External {
  pub fn new(mut argv: Vec<String>) -> Self {
    let name = argv.remove(0);
    Self { name, args: argv }
  }

  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    if let Some(project) = ctx.try_project()?
      && project.manifest.scripts.contains_key(&self.name)
    {
      project.ensure_compatible(&Context::version())?;
      return RunScriptArgs::exec(ctx, &project, &self.name, &self.args);
    }
    let binary = format!("wkit-{}", self.name);
    let Ok(path) = which::which(&binary) else {
      anyhow::bail!(
        "no such command or script: `{}`\n\nView all commands with `wkit --help` and scripts with `wkit x`",
        self.name
      );
    };
    let mut process = ProcessBuilder::new(path).args(&self.args).cwd(&ctx.cwd);
    if let Ok(exe) = std::env::current_exe() {
      process = process.env("WKIT", exe);
    }
    let status = process.status_unchecked()?;
    if status.success() {
      Ok(())
    } else {
      Err(Failure::silent(status.code().unwrap_or(Failure::INTERNAL)).into())
    }
  }
}
