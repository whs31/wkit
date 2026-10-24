use std::{
  num::NonZeroUsize,
  path::{Path, PathBuf},
};

use anyhow::Context as _;

use super::{Run, exit_status, jobs};
use crate::{
  Failure, ProcessBuilder, Result,
  build::{BuildOptions, Builder, ConanWorkspace, TEST_PACKAGE},
  check::{CheckOptions, Checker, ClangTidy},
  cli::Context,
  format::{Formatter, Mode},
  manifest::Project,
  shell::plural,
  version::{Bump, Change, ProjectVersion, VersionSet},
};

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

impl Run for BumpArgs {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
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
      for mismatch in set.mismatches(&current) {
        ctx.shell.warn(mismatch)?;
      }
      return Ok(());
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
      let tag = git
        .clone()
        .args(["rev-parse", "--quiet", "--verify"])
        .arg(format!("refs/tags/{target}"));
      anyhow::ensure!(
        !tag.output_unchecked()?.status.success(),
        "tag `{target}` already exists"
      );
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
      commit_version(ctx, &git, &changes, &target, self.tag)?;
    }
    writeln!(ctx.shell.out(), "{target}")?;
    Ok(())
  }
}

impl BumpArgs {
  fn target(&self, current: &ProjectVersion, set: &VersionSet) -> Result<Option<ProjectVersion>> {
    if let Some(version) = &self.assign {
      return Ok(Some(ProjectVersion::parse(version, set.scheme())?));
    }
    let level = [
      (self.major, Bump::Major),
      (self.minor, Bump::Minor),
      (self.patch, Bump::Patch),
    ]
    .into_iter()
    .find_map(|(set, level)| set.then_some(level));
    Ok(match (&self.pre, level) {
      (Some(id), level) => Some(current.bumped_pre(level.unwrap_or(Bump::Patch), id)?),
      (None, Some(level)) => Some(current.bumped(level)),
      (None, None) => None,
    })
  }
}

fn commit_version(
  ctx: &mut Context<'_>,
  git: &ProcessBuilder,
  changes: &[Change],
  target: &ProjectVersion,
  tag: bool,
) -> Result<()> {
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

#[derive(Debug, clap::Args)]
pub struct FmtArgs {
  /// Report unformatted files instead of formatting them
  #[arg(long)]
  check: bool,

  /// Number of parallel formatter jobs
  #[arg(short, long, value_name = "N")]
  jobs: Option<NonZeroUsize>,
}

impl Run for FmtArgs {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    let project = ctx.project()?;
    let formatter = Formatter::new(&project)?;
    if formatter.is_empty() {
      ctx
        .shell
        .warn("nothing to format: no formatter in `[format]` is enabled")?;
      return Ok(());
    }
    let start = std::time::Instant::now();
    let mode = if self.check { Mode::Check } else { Mode::Write };
    let jobs = jobs(self.jobs).unwrap_or_else(|| CheckOptions::default().jobs);
    let files = formatter.run(ctx.shell, mode, jobs)?;
    match mode {
      Mode::Write => {
        for file in &files {
          ctx.shell.status("Formatted", file.display())?;
        }
        ctx.shell.finished("formatting", start.elapsed())?;
      }
      Mode::Check if files.is_empty() => ctx.shell.finished("format check", start.elapsed())?,
      Mode::Check => {
        let list: Vec<_> = files.iter().map(|f| f.display().to_string()).collect();
        let verb = if files.len() == 1 { "needs" } else { "need" };
        let message = format!(
          "{} {verb} formatting; run `wkit fmt`:\n  {}",
          plural(files.len(), "file"),
          list.join("\n  ")
        );
        return Err(Failure::new(message, 1).into());
      }
    }
    Ok(())
  }
}

#[derive(Debug, clap::Args)]
pub struct CheckArgs {
  /// Run only these stages (format, versions, clippy, clang-tidy, secrets, whitespace, pre-commit or a custom check)
  #[arg(long, value_name = "STAGE", value_delimiter = ',')]
  only: Vec<String>,

  /// Skip these stages
  #[arg(long, value_name = "STAGE", value_delimiter = ',')]
  skip: Vec<String>,

  /// Number of parallel clang-format and clang-tidy jobs
  #[arg(short, long, value_name = "N")]
  jobs: Option<NonZeroUsize>,

  /// Ignore cached clang-tidy results
  #[arg(long)]
  no_cache: bool,

  /// Build directory holding `compile_commands.json`
  #[arg(value_name = "BUILD_DIR")]
  build_dir: Option<PathBuf>,
}

impl Run for CheckArgs {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    let project = ctx.project()?;
    let checker = Checker::plan(&project, &self.only, &self.skip)?;
    if checker.names().is_empty() {
      ctx.shell.warn("no checks to run")?;
      return Ok(());
    }
    let jobs = jobs(self.jobs);
    let mut build_dir = self.build_dir.map(|dir| ctx.cwd.join(dir));
    if build_dir.is_none()
      && checker.names().iter().any(|n| n == ClangTidy::NAME)
      && !ClangTidy::is_configured(&project.root)
    {
      let options = BuildOptions {
        tests: true,
        jobs,
        ..BuildOptions::default()
      };
      build_dir = Builder::new(&project, &options)
        .run_native(ctx.shell)?
        .and_then(|outcome| outcome.build_dir().map(ToOwned::to_owned));
    }
    let defaults = CheckOptions::default();
    let options = CheckOptions {
      jobs: jobs.unwrap_or(defaults.jobs),
      cache: !self.no_cache,
      build_dir,
      verbose: ctx.shell.is_verbose(),
    };
    let start = std::time::Instant::now();
    let reports = checker.run(ctx.shell, &options)?;
    let failed: Vec<_> = reports
      .iter()
      .filter(|r| r.failed())
      .map(|r| r.name.as_str())
      .collect();
    if !failed.is_empty() {
      return Err(Failure::new(format!("checks failed: {}", failed.join(", ")), 1).into());
    }
    ctx.shell.finished("checks", start.elapsed())?;
    Ok(())
  }
}

#[derive(Debug, clap::Args)]
pub struct CleanArgs {
  /// Show what would be removed without removing anything
  #[arg(long)]
  dry_run: bool,
}

impl Run for CleanArgs {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    let project = ctx.project()?;
    let mut roots = vec![project.root.clone()];
    if let Some(workspace) = ConanWorkspace::load(&project.root)? {
      roots.extend(
        workspace
          .packages
          .iter()
          .map(|p| project.join(&p.path))
          .filter(|dir| dir.is_dir()),
      );
    }
    let test_packages: Vec<_> = roots
      .iter()
      .map(|root| root.join(TEST_PACKAGE))
      .filter(|dir| dir.join("conanfile.py").is_file())
      .collect();
    roots.extend(test_packages);
    let mut targets = Vec::new();
    for raw in project
      .manifest
      .clean
      .effective_paths(project.manifest.languages())
    {
      let pattern = PathPattern::new(&raw)?;
      for root in &roots {
        targets.extend(pattern.expand(root)?);
      }
    }
    targets.sort();
    let mut kept: Vec<PathBuf> = Vec::with_capacity(targets.len());
    for target in targets {
      if !kept.last().is_some_and(|parent| target.starts_with(parent)) {
        kept.push(target);
      }
    }

    let (mut files, mut bytes) = (0, 0);
    for target in &kept {
      let (f, b) = measure(target)?;
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
        let removed = if fs_err::symlink_metadata(target)?.is_dir() {
          fs_err::remove_dir_all(target)
        } else {
          fs_err::remove_file(target)
        };
        removed?;
      }
    }
    let summary = format!("{}, {} total", plural(files, "file"), human_bytes(bytes));
    if self.dry_run {
      ctx.shell.status("Summary", &summary)?;
      ctx.shell.warn("no files deleted due to --dry-run")?;
    } else {
      ctx.shell.status("Removed", &summary)?;
    }
    Ok(())
  }
}

fn measure(path: &Path) -> Result<(usize, u64)> {
  let meta = fs_err::symlink_metadata(path)?;
  if !meta.is_dir() {
    return Ok((1, meta.len()));
  }
  let mut total = (0, 0);
  for entry in fs_err::read_dir(path)? {
    let (files, bytes) = measure(&entry?.path())?;
    total.0 += files;
    total.1 += bytes;
  }
  Ok(total)
}

#[allow(clippy::cast_precision_loss)]
fn human_bytes(bytes: u64) -> String {
  const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
  if bytes < 1024 {
    return format!("{bytes}B");
  }
  let mut value = bytes as f64 / 1024.0;
  let mut unit = 0;
  while value >= 1024.0 && unit + 1 < UNITS.len() {
    value /= 1024.0;
    unit += 1;
  }
  format!("{value:.1}{}", UNITS[unit])
}

#[derive(Debug, Clone)]
enum Segment {
  Literal(String),
  Glob(globset::GlobMatcher),
}

#[derive(Debug, Clone)]
struct PathPattern {
  raw: String,
  segments: Vec<Segment>,
}

impl PathPattern {
  fn new(raw: &str) -> Result<Self> {
    let invalid = |why: &str| anyhow::anyhow!("invalid clean path `{raw}`: {why}");
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.starts_with('/') || Path::new(trimmed).is_absolute() {
      return Err(invalid("it must be relative to the project root"));
    }
    let mut segments = Vec::new();
    for part in trimmed.split('/') {
      match part {
        "" | "." => {}
        ".." => return Err(invalid("it must stay inside the project")),
        p if p.contains("**") => return Err(invalid("`**` is not supported")),
        p if p.contains(['*', '?', '[', '{']) => {
          let glob = globset::GlobBuilder::new(p)
            .literal_separator(true)
            .build()
            .map_err(|e| invalid(&e.to_string()))?;
          segments.push(Segment::Glob(glob.compile_matcher()));
        }
        p => segments.push(Segment::Literal(p.to_owned())),
      }
    }
    if segments.is_empty() {
      return Err(invalid("it would remove the whole project"));
    }
    Ok(Self {
      raw: raw.to_owned(),
      segments,
    })
  }

  fn expand(&self, root: &Path) -> Result<Vec<PathBuf>> {
    let mut frontier = vec![root.to_owned()];
    for segment in &self.segments {
      let mut next = Vec::new();
      for dir in &frontier {
        match segment {
          Segment::Literal(name) => {
            let path = dir.join(name);
            if std::fs::symlink_metadata(&path).is_ok() {
              next.push(path);
            }
          }
          Segment::Glob(glob) => {
            let Ok(entries) = std::fs::read_dir(dir) else {
              continue;
            };
            for entry in entries {
              let entry = entry.with_context(|| format!("failed to expand `{}`", self.raw))?;
              if glob.is_match(entry.file_name()) {
                next.push(entry.path());
              }
            }
          }
        }
      }
      frontier = next;
    }
    frontier.sort();
    Ok(frontier)
  }
}

#[derive(Debug, clap::Args)]
pub struct RunScriptArgs {
  /// Script to run; lists the scripts when omitted
  name: Option<String>,

  /// Arguments appended to the script
  #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
  args: Vec<String>,
}

impl Run for RunScriptArgs {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    let project = ctx.project()?;
    if let Some(name) = self.name {
      return run_script(ctx, &project, &name, &self.args);
    }
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
}

fn run_script(ctx: &mut Context<'_>, project: &Project, name: &str, args: &[String]) -> Result<()> {
  let scripts = &project.manifest.scripts;
  let Some(task) = scripts.get(name) else {
    let names: Vec<_> = scripts.keys().map(String::as_str).collect();
    let available = if names.is_empty() {
      String::new()
    } else {
      format!("; available: {}", names.join(", "))
    };
    anyhow::bail!(
      "no script named `{name}` in `{}`{available}",
      project.manifest_path.display()
    );
  };
  ctx
    .shell
    .status("Running", format_args!("`{}`", task.run.line(args)))?;
  let status = task.to_process(&project.root, args).status_unchecked()?;
  if status.success() {
    return Ok(());
  }
  let code = status.code().unwrap_or(Failure::INTERNAL);
  Err(Failure::new(format!("script `{name}` failed ({status})"), code).into())
}

pub fn external(ctx: &mut Context<'_>, mut argv: Vec<String>) -> Result<()> {
  let name = argv.remove(0);
  if let Some(project) = ctx.try_project()?
    && project.manifest.scripts.contains_key(&name)
  {
    project.ensure_compatible(&Context::version())?;
    return run_script(ctx, &project, &name, &argv);
  }
  let Ok(path) = which::which(format!("wkit-{name}")) else {
    anyhow::bail!(
      "no such command or script: `{name}`\n\nView all commands with `wkit --help` and scripts with `wkit x`"
    );
  };
  let mut process = ProcessBuilder::new(path).args(&argv).cwd(&ctx.cwd);
  if let Ok(exe) = std::env::current_exe() {
    process = process.env("WKIT", exe);
  }
  exit_status(process.status_unchecked()?)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn expands_literals_and_globs_under_the_root() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for path in [
      "build/x",
      "cmake-build-debug/y",
      "cmake-build-release/z",
      "docs/graph.html",
    ] {
      std::fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
      std::fs::write(root.join(path), "").unwrap();
    }
    let expand = |p: &str| -> Vec<String> {
      let found = PathPattern::new(p).unwrap().expand(root).unwrap();
      found
        .iter()
        .map(|p| {
          p.strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/")
        })
        .collect()
    };
    assert_eq!(expand("build/"), ["build"]);
    assert_eq!(
      expand("cmake-build-*"),
      ["cmake-build-debug", "cmake-build-release"]
    );
    assert_eq!(expand("docs/graph.html"), ["docs/graph.html"]);
    assert_eq!(expand("*/z"), ["cmake-build-release/z"]);
    assert_eq!(expand("target"), Vec::<String>::new());
  }

  #[test]
  fn rejects_paths_outside_the_project() {
    for raw in ["/etc", "../x", "a/../../b", ".", "", "**/build"] {
      assert!(PathPattern::new(raw).is_err(), "{raw}");
    }
  }

  #[test]
  fn sizes_are_human_readable() {
    for (bytes, shown) in [
      (0, "0B"),
      (1023, "1023B"),
      (1536, "1.5KiB"),
      (3 << 30, "3.0GiB"),
    ] {
      assert_eq!(human_bytes(bytes), shown);
    }
  }
}
