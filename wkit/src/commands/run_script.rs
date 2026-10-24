use wkit_manifest::Project;

use crate::{context::Context, failure::Failure};

#[derive(Debug, clap::Args)]
pub struct RunScriptArgs {
  /// Script to run; lists the scripts when omitted
  name: Option<String>,

  /// Arguments appended to the script
  #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
  args: Vec<String>,
}

impl RunScriptArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let project = ctx.project()?;
    match self.name {
      Some(name) => Self::exec(ctx, &project, &name, &self.args),
      None => Self::list(ctx, &project),
    }
  }

  pub fn exec(
    ctx: &mut Context<'_>,
    project: &Project,
    name: &str,
    args: &[String],
  ) -> anyhow::Result<()> {
    let Some(task) = project.manifest.scripts.get(name) else {
      anyhow::bail!(
        "no script named `{name}` in `{}`{}",
        project.manifest_path.display(),
        Self::available(project)
      );
    };
    let process = task.to_process_with(&project.root, args);
    ctx
      .shell
      .status("Running", format_args!("`{}`", task.run.line_with(args)))?;
    let status = process.status_unchecked()?;
    if status.success() {
      Ok(())
    } else {
      Err(
        Failure::new(
          format!("script `{name}` failed ({status})"),
          status.code().unwrap_or(Failure::INTERNAL),
        )
        .into(),
      )
    }
  }

  fn list(ctx: &mut Context<'_>, project: &Project) -> anyhow::Result<()> {
    let scripts = &project.manifest.scripts;
    if scripts.is_empty() {
      ctx
        .shell
        .note("no scripts defined; add them under `[scripts]`")?;
      return Ok(());
    }
    let width = scripts.keys().map(String::len).max().unwrap_or_default();
    let out = ctx.shell.out();
    for (name, task) in scripts {
      match &task.description {
        Some(description) => writeln!(out, "{name:<width$}  {description}")?,
        None => writeln!(out, "{name}")?,
      }
    }
    Ok(())
  }

  fn available(project: &Project) -> String {
    let names: Vec<_> = project
      .manifest
      .scripts
      .keys()
      .map(String::as_str)
      .collect();
    if names.is_empty() {
      String::new()
    } else {
      format!("; available: {}", names.join(", "))
    }
  }
}
