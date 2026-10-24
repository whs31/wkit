mod path_pattern;

use wkit_core::shell::Bytes;

use self::path_pattern::PathPattern;
use crate::context::Context;

#[derive(Debug, clap::Args)]
pub struct CleanArgs {
  /// Show what would be removed without removing anything
  #[arg(long)]
  dry_run: bool,
}

impl CleanArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let project = ctx.project()?;
    let mut targets = Vec::new();
    for raw in project
      .manifest
      .clean
      .effective_paths(project.manifest.languages())
    {
      targets.extend(PathPattern::new(&raw)?.expand(&project.root)?);
    }
    targets.sort();
    let mut kept: Vec<std::path::PathBuf> = Vec::with_capacity(targets.len());
    for target in targets {
      if !kept.last().is_some_and(|parent| target.starts_with(parent)) {
        kept.push(target);
      }
    }

    let (mut files, mut bytes) = (0, 0);
    for target in &kept {
      let (f, b) = Self::measure(target)?;
      files += f;
      bytes += b;
      let shown = target
        .strip_prefix(&project.root)
        .unwrap_or(target)
        .display();
      if self.dry_run {
        ctx.shell.status("Removing", shown)?;
      } else {
        ctx.shell.verbose(|s| s.status("Removing", shown))?;
        Self::remove(target)?;
      }
    }
    let summary = format!(
      "{files} file{}, {} total",
      if files == 1 { "" } else { "s" },
      Bytes(bytes)
    );
    if self.dry_run {
      ctx.shell.status("Summary", &summary)?;
      ctx.shell.warn("no files deleted due to --dry-run")?;
    } else {
      ctx.shell.status("Removed", &summary)?;
    }
    Ok(())
  }

  fn measure(path: &std::path::Path) -> anyhow::Result<(u64, u64)> {
    let context = || format!("failed to inspect `{}`", path.display());
    let meta =
      std::fs::symlink_metadata(path).map_err(|e| anyhow::Error::from(e).context(context()))?;
    if !meta.is_dir() {
      return Ok((1, meta.len()));
    }
    let mut total = (0, 0);
    for entry in std::fs::read_dir(path).map_err(|e| anyhow::Error::from(e).context(context()))? {
      let entry = entry.map_err(|e| anyhow::Error::from(e).context(context()))?;
      let (files, bytes) = Self::measure(&entry.path())?;
      total.0 += files;
      total.1 += bytes;
    }
    Ok(total)
  }

  fn remove(path: &std::path::Path) -> anyhow::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    let result = if meta.is_dir() {
      std::fs::remove_dir_all(path)
    } else {
      std::fs::remove_file(path).or_else(|_| std::fs::remove_dir(path))
    };
    result
      .map_err(|e| anyhow::Error::from(e).context(format!("failed to remove `{}`", path.display())))
  }
}
