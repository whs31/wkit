mod detection;

use wkit_manifest::{MANIFEST_FILE, Manifest};

use self::detection::Detection;
use crate::context::Context;

#[derive(Debug, clap::Args)]
pub struct InitArgs {
  /// Overwrite an existing wkit.toml
  #[arg(long)]
  force: bool,
}

impl InitArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let path = match &ctx.manifest_path {
      Some(path) => ctx.cwd.join(path),
      None => ctx.cwd.join(MANIFEST_FILE),
    };
    anyhow::ensure!(
      self.force || !path.exists(),
      "`{}` already exists; pass `--force` to overwrite it",
      path.display()
    );
    let root = path.parent().unwrap_or(&ctx.cwd);
    let detection = Detection::scan(root)?;
    let text = detection.render();
    Manifest::parse(&text).map_err(|e| {
      anyhow::Error::from(e).context("generated an invalid manifest; this is a bug")
    })?;
    wkit_core::fs::write(&path, &text)?;
    let languages: Vec<_> = detection
      .languages
      .iter()
      .map(ToString::to_string)
      .collect();
    ctx.shell.status(
      "Created",
      format_args!(
        "`{}` ({}; {} version file{})",
        path.strip_prefix(&ctx.cwd).unwrap_or(&path).display(),
        languages.join(", "),
        detection.version_files.len(),
        if detection.version_files.len() == 1 {
          ""
        } else {
          "s"
        }
      ),
    )?;
    Ok(())
  }
}
