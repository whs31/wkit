mod host_info;
mod project_info;

pub use self::{host_info::HostInfo, project_info::ProjectInfo};
use crate::context::Context;

#[derive(Debug, clap::Args)]
pub struct InfoArgs {
  /// Print a JSON object instead of text
  #[arg(long)]
  json: bool,
}

impl InfoArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let host = HostInfo::detect();
    let project = ctx
      .try_project()?
      .map(|project| ProjectInfo::collect(&project))
      .transpose()?;
    let out = ctx.shell.out();
    if self.json {
      let info = serde_json::json!({
        "wkit": env!("CARGO_PKG_VERSION"),
        "host": host,
        "project": project,
      });
      writeln!(out, "{}", serde_json::to_string_pretty(&info)?)?;
      return Ok(());
    }
    let mut sections = vec![host.fields()];
    sections.extend(project.as_ref().map(ProjectInfo::fields));
    let width = sections
      .iter()
      .flatten()
      .map(|(key, _)| key.len() + 1)
      .max()
      .unwrap_or_default();
    writeln!(out, "wkit {}", env!("CARGO_PKG_VERSION"))?;
    for section in sections {
      writeln!(out)?;
      for (key, value) in section {
        writeln!(out, "{:<width$} {value}", format!("{key}:"))?;
      }
    }
    Ok(())
  }
}
