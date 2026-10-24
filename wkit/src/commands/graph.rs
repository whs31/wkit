use wkit_build::{Builder, Conan};

use super::build_selection::BuildSelection;
use crate::context::Context;

#[derive(Debug, clap::Args)]
pub struct GraphArgs {
  #[command(flatten)]
  selection: BuildSelection,

  /// Where to write the HTML graph
  #[arg(short, long, value_name = "PATH", default_value = "docs/graph.html")]
  output: std::path::PathBuf,

  /// Open the graph in the default browser
  #[arg(long)]
  open: bool,
}

impl GraphArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let project = ctx.project()?;
    if !project.join("conanfile.py").is_file() {
      anyhow::bail!("`wkit graph` needs a conanfile.py at the project root");
    }
    let options = self.selection.options();
    let build_type = Builder::new(&project, &options).build_type()?;
    let output = project.join(&self.output);
    if let Some(parent) = output.parent() {
      std::fs::create_dir_all(parent)?;
    }
    Conan::new(&project, &options).graph(ctx.shell, build_type, &output)?;
    ctx.shell.status("Generated", self.output.display())?;
    if self.open {
      Self::opener(&output).status()?;
    }
    Ok(())
  }

  fn opener(path: &std::path::Path) -> wkit_core::ProcessBuilder {
    if cfg!(windows) {
      wkit_core::ProcessBuilder::new("cmd")
        .args(["/C", "start", ""])
        .arg(path)
    } else if cfg!(target_os = "macos") {
      wkit_core::ProcessBuilder::new("open").arg(path)
    } else {
      wkit_core::ProcessBuilder::new("xdg-open").arg(path)
    }
  }
}
