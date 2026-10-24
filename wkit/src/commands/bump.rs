use wkit_core::{
  ProcessBuilder,
  version::{Bump, ProjectVersion},
};
use wkit_version::{Change, VersionSet};

use crate::context::Context;

#[derive(Debug, clap::Args)]
#[allow(clippy::struct_excessive_bools)]
#[command(group = clap::ArgGroup::new("level").args(["major", "minor", "patch", "assign"]))]
pub struct BumpArgs {
  /// Bump the major version (1.2.3 -> 2.0.0)
  #[arg(short = 'M', long)]
  major: bool,

  /// Bump the minor version (1.2.3 -> 1.3.0)
  #[arg(short, long)]
  minor: bool,

  /// Bump the patch version (1.2.3 -> 1.2.4)
  #[arg(short, long)]
  patch: bool,

  /// Set an exact version
  #[arg(short, long, value_name = "VERSION")]
  assign: Option<String>,

  /// Start or advance a prerelease (1.2.3 -> 1.2.4-rc.1 -> 1.2.4-rc.2)
  #[arg(long, value_name = "ID", conflicts_with = "assign")]
  pre: Option<String>,

  /// Show what would change without writing anything
  #[arg(long)]
  dry_run: bool,

  /// Commit the version files as `v<version>`
  #[arg(long)]
  commit: bool,

  /// Tag the commit as `<version>`
  #[arg(long, requires = "commit")]
  tag: bool,
}

impl BumpArgs {
  pub fn run(self, ctx: &mut Context<'_>) -> anyhow::Result<()> {
    let project = ctx.project()?;
    let Some(section) = &project.manifest.version else {
      anyhow::bail!(
        "`{}` has no `[version]` section; list the files that carry the version in `version.files`",
        project.manifest_path.display()
      );
    };
    let set = VersionSet::load(&project.root, section)?;
    let current = set.current()?;
    let Some(target) = self.target(&current, &set)? else {
      return Self::show(ctx, &set, &current);
    };
    for mismatch in set.mismatches(&current) {
      ctx.shell.warn(mismatch)?;
    }
    if target == current {
      ctx
        .shell
        .note(format_args!("{} is already at {current}", project.name()))?;
      return Ok(());
    }
    let git = ProcessBuilder::new("git").cwd(&project.root);
    if self.tag && !self.dry_run {
      let exists = git
        .clone()
        .args(["rev-parse", "--quiet", "--verify"])
        .arg(format!("refs/tags/{target}"))
        .output_unchecked()?;
      anyhow::ensure!(!exists.status.success(), "tag `{target}` already exists");
    }

    let changes = set.changes(&target);
    ctx.shell.status(
      "Bumping",
      format_args!("{} {current} -> {target}", project.name()),
    )?;
    for change in &changes {
      let line = format!(
        "{} ({} -> {})",
        change.path.display(),
        change.from,
        change.to
      );
      if self.dry_run {
        ctx.shell.status("Updating", &line)?;
      } else {
        ctx.shell.verbose(|s| s.status("Updating", &line))?;
      }
    }
    if self.dry_run {
      ctx.shell.warn("aborting bump due to dry run")?;
      return Ok(());
    }
    for change in &changes {
      change.apply()?;
    }
    if self.commit {
      Self::commit(ctx, &git, &changes, &target, self.tag)?;
    }
    writeln!(ctx.shell.out(), "{target}")?;
    Ok(())
  }

  fn target(
    &self,
    current: &ProjectVersion,
    set: &VersionSet,
  ) -> anyhow::Result<Option<ProjectVersion>> {
    if let Some(version) = &self.assign {
      return Ok(Some(ProjectVersion::parse(version, set.scheme())?));
    }
    let level = if self.major {
      Some(Bump::Major)
    } else if self.minor {
      Some(Bump::Minor)
    } else if self.patch {
      Some(Bump::Patch)
    } else {
      None
    };
    Ok(match (&self.pre, level) {
      (Some(id), level) => Some(current.bumped_pre(level.unwrap_or(Bump::Patch), id)?),
      (None, Some(level)) => Some(current.bumped(level)),
      (None, None) => None,
    })
  }

  fn show(ctx: &mut Context<'_>, set: &VersionSet, current: &ProjectVersion) -> anyhow::Result<()> {
    let width = set
      .files()
      .iter()
      .map(|f| f.path.display().to_string().len())
      .max()
      .unwrap_or_default();
    for file in set.files() {
      writeln!(
        ctx.shell.out(),
        "{:<width$}  {}",
        file.path.display(),
        file.current()
      )?;
    }
    for mismatch in set.mismatches(current) {
      ctx.shell.warn(mismatch)?;
    }
    Ok(())
  }

  fn commit(
    ctx: &mut Context<'_>,
    git: &ProcessBuilder,
    changes: &[Change],
    target: &ProjectVersion,
    tag: bool,
  ) -> anyhow::Result<()> {
    let message = format!("v{target}");
    ctx.shell.status("Committing", &message)?;
    git
      .clone()
      .args(["commit", "--quiet", "--message", &message, "--"])
      .args(changes.iter().map(|c| &c.path))
      .output()?;
    if tag {
      ctx.shell.status("Tagging", target)?;
      git.clone().arg("tag").arg(target.to_string()).output()?;
    }
    Ok(())
  }
}
