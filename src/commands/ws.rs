use std::{
  path::{Path, PathBuf},
  time::Instant,
};

use super::Run;
use crate::{
  Failure, ProcessBuilder, Result, Shell,
  cli::Context,
  ws::{
    self, Committer, GITIGNORE, LOCK_FILE, MANIFEST_FILE, Member, TaskOutcome, Workspace, parallel,
  },
};

#[derive(Debug, clap::Subcommand)]
pub enum WsCommand {
  /// Clone missing projects at the locked snapshot
  Init(Jobs),

  /// Return clean projects to the locked snapshot
  Restore(Jobs),

  /// Fast-forward projects along their development branches
  Update {
    #[command(flatten)]
    jobs: Jobs,

    /// With `--commit`: relock and commit `workspace.lock`
    #[command(flatten)]
    publish: Publish,
  },

  /// Record every clean project's HEAD in `workspace.lock`
  Lock {
    #[command(flatten)]
    jobs: Jobs,

    #[command(flatten)]
    publish: Publish,
  },

  /// Show branch, worktree, upstream and lock state
  Status(Jobs),

  /// Clone a new project and register it
  Add {
    /// Git remote URL
    url: String,

    /// Workspace path [default: derived from the URL and the existing layout]
    path: Option<String>,

    /// Development branch [default: the remote's HEAD branch]
    #[arg(short, long, value_name = "BRANCH")]
    branch: Option<String>,

    #[command(flatten)]
    publish: Publish,
  },

  /// Commit pending project formatting or `workspace.lock`
  #[command(subcommand_value_name = "WHAT")]
  Commit {
    #[command(subcommand)]
    what: CommitWhat,

    /// Number of projects to process in parallel
    #[arg(short, long, global = true, value_name = "N", default_value_t = ws::DEFAULT_JOBS)]
    jobs: usize,

    /// Push the new commits
    #[arg(long, global = true)]
    push: bool,
  },

  /// Run a command in every cloned project
  Exec {
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
  },

  /// Format every project (`wkit fmt`, else its `just fmt`)
  Fmt {
    #[command(flatten)]
    jobs: Jobs,

    /// With `--commit`: commit the changes as `CHORE: Formatting` in each project
    #[command(flatten)]
    publish: Publish,
  },
}

#[derive(Debug, Clone, Copy, clap::Subcommand)]
pub enum CommitWhat {
  /// Commit every project's uncommitted tracked changes as `CHORE: Formatting`
  Fmt,

  /// Commit `workspace.lock` as `CHORE: Update lock`, listing what moved
  Lock,
}

#[derive(Debug, Clone, Copy, clap::Args)]
pub struct Jobs {
  /// Number of projects to process in parallel
  #[arg(short, long, value_name = "N", default_value_t = ws::DEFAULT_JOBS)]
  jobs: usize,
}

#[derive(Debug, Clone, Copy, clap::Args)]
pub struct Publish {
  /// Commit the result
  #[arg(long)]
  commit: bool,

  /// Push the commit as well
  #[arg(long, requires = "commit")]
  push: bool,
}

impl Run for WsCommand {
  fn run(self, ctx: &mut Context<'_>) -> Result<()> {
    let start = Instant::now();
    let mut workspace = Workspace::discover(&ctx.cwd)?;
    let shell = &mut *ctx.shell;
    match self {
      Self::Init(Jobs { jobs }) => {
        let locked = workspace.is_fully_locked();
        workspace
          .init(shell, jobs)?
          .finish(shell, "cloned", start)?;
        if !locked {
          workspace.relock(shell, jobs)?;
        }
      }
      Self::Restore(Jobs { jobs }) => workspace
        .restore(shell, jobs)?
        .finish(shell, "restored", start)?,
      Self::Update {
        jobs: Jobs { jobs },
        publish,
      } => {
        workspace
          .update(shell, jobs)?
          .finish(shell, "updated", start)?;
        if publish.commit {
          workspace.relock(shell, jobs)?;
          Committer::new(&workspace).commit_lock(shell, publish.push, jobs)?;
        }
      }
      Self::Lock {
        jobs: Jobs { jobs },
        publish,
      } => {
        workspace.relock(shell, jobs)?;
        if publish.commit {
          Committer::new(&workspace).commit_lock(shell, publish.push, jobs)?;
        }
      }
      Self::Status(Jobs { jobs }) => {
        let statuses = workspace.status(jobs);
        let width = statuses
          .iter()
          .map(|s| s.branch.as_deref().unwrap_or("detached").len())
          .max()
          .unwrap_or_default();
        for status in &statuses {
          writeln!(shell.out(), "{}", status.render(width))?;
        }
        if statuses.iter().any(|s| s.broken) {
          return Err(Failure::silent(1).into());
        }
      }
      Self::Add {
        url,
        path,
        branch,
        publish,
      } => {
        let member = workspace.add(shell, &url, path.as_deref(), branch.as_deref())?;
        if publish.commit {
          let message = format!("FEAT: Add {}", member.name());
          Committer::new(&workspace).commit_root(
            shell,
            &message,
            &[MANIFEST_FILE, LOCK_FILE, GITIGNORE],
            publish.push,
          )?;
        }
      }
      Self::Commit {
        what: CommitWhat::Lock,
        jobs,
        push,
      } => Committer::new(&workspace).commit_lock(shell, push, jobs)?,
      Self::Commit {
        what: CommitWhat::Fmt,
        jobs,
        push,
      } => {
        let changed = parallel(&workspace.manifest.members, jobs, |m| {
          let repo = workspace.repo(m);
          repo.is_standalone() && !repo.changed_files().is_empty()
        });
        let members = select(&workspace.manifest.members, &changed);
        commit_formatting(shell, &workspace, &members, jobs, push, start)?;
      }
      Self::Exec { fail_fast, command } => exec(shell, &workspace, fail_fast, &command, start)?,
      Self::Fmt {
        jobs: Jobs { jobs },
        publish,
      } => format(shell, &workspace, jobs, publish, start)?,
    }
    Ok(())
  }
}

fn select(members: &[Member], keep: &[bool]) -> Vec<Member> {
  members
    .iter()
    .zip(keep)
    .filter(|(_, keep)| **keep)
    .map(|(m, _)| m.clone())
    .collect()
}

fn commit_formatting(
  shell: &mut Shell,
  workspace: &Workspace,
  members: &[Member],
  jobs: usize,
  push: bool,
  start: Instant,
) -> Result<()> {
  if members.is_empty() {
    shell.status("Fresh", "no formatting changes to commit")?;
    return Ok(());
  }
  let committer = Committer::new(workspace);
  let report = ws::run(
    shell,
    jobs,
    ("Committing", "Committed"),
    members,
    |member, phase| committer.commit_member(member, Committer::FORMATTING, push, phase),
  )?;
  report.finish(shell, if push { "pushed" } else { "committed" }, start)
}

fn exec(
  shell: &mut Shell,
  workspace: &Workspace,
  fail_fast: bool,
  command: &[String],
  start: Instant,
) -> Result<()> {
  let members = workspace.cloned();
  if members.is_empty() {
    shell.warn("no cloned projects (run `wkit ws init`)")?;
    return Ok(());
  }
  let shown = command.join(" ");
  let mut failed = Vec::new();
  for member in &members {
    shell.status("Running", format_args!("`{shown}` in {}", member.path))?;
    let process = match command {
      [line] => ProcessBuilder::shell(line),
      [program, args @ ..] => ProcessBuilder::new(program).args(args),
      [] => unreachable!("clap requires a command"),
    };
    let status = process
      .cwd(member.dir(&workspace.root))
      .status_unchecked()?;
    if !status.success() {
      shell.error(format_args!("{}: {status}", member.path))?;
      failed.push(member.path.as_str());
      if fail_fast {
        break;
      }
    }
  }
  if !failed.is_empty() {
    let message = format!(
      "{} of {} projects failed: {}",
      failed.len(),
      members.len(),
      failed.join(", ")
    );
    return Err(Failure::new(message, 1).into());
  }
  shell.finished(format_args!("{} projects", members.len()), start.elapsed())?;
  Ok(())
}

fn format(
  shell: &mut Shell,
  workspace: &Workspace,
  jobs: usize,
  publish: Publish,
  start: Instant,
) -> Result<()> {
  let detected = parallel(&workspace.manifest.members, jobs, |m| {
    FmtTarget::detect(&m.dir(&workspace.root))
  });
  let (members, targets): (Vec<Member>, Vec<FmtTarget>) = workspace
    .manifest
    .members
    .iter()
    .cloned()
    .zip(detected)
    .filter_map(|(m, t)| t.map(|t| (m, t)))
    .unzip();
  if members.is_empty() {
    shell
      .warn("no project has a `wkit.toml` or a `fmt`/`format` just recipe (are they cloned?)")?;
    return Ok(());
  }
  let has_changes = |m: &Member| !workspace.repo(m).changed_files().is_empty();
  let dirty = if publish.commit {
    parallel(&members, jobs, has_changes)
  } else {
    vec![false; members.len()]
  };
  let wkit = std::env::current_exe()?;
  let report = ws::run(
    shell,
    jobs,
    ("Formatting", "Formatted"),
    &members,
    |member, phase| {
      let target = &targets[members
        .iter()
        .position(|m| m == member)
        .expect("member is listed")];
      phase.set(&target.label(), None);
      match target
        .command(&member.dir(&workspace.root), &wkit)
        .output_unchecked()
      {
        Ok(output) if output.status.success() => {
          TaskOutcome::done(format!("via `{}`", target.label()))
        }
        Ok(output) => TaskOutcome {
          output: format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
          ),
          ..TaskOutcome::failed(format!("`{}` failed ({})", target.label(), output.status))
        },
        Err(e) => TaskOutcome::failed(e.to_string()),
      }
    },
  )?;
  report.finish(shell, "formatted", start)?;
  if !publish.commit {
    return Ok(());
  }
  for member in select(&members, &dirty) {
    shell.warn(format_args!(
      "{} had uncommitted changes before formatting, not committing it",
      member.path
    ))?;
  }
  let clean: Vec<bool> = members
    .iter()
    .zip(&dirty)
    .map(|(m, dirty)| !dirty && has_changes(m))
    .collect();
  commit_formatting(
    shell,
    workspace,
    &select(&members, &clean),
    jobs,
    publish.push,
    start,
  )
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FmtTarget {
  Wkit,
  Just { justfile: PathBuf, recipe: String },
}

impl FmtTarget {
  const RECIPES: &[&str] = &["fmt", "format"];

  fn detect(dir: &Path) -> Option<Self> {
    if dir.join("wkit.toml").is_file() {
      return Some(Self::Wkit);
    }
    let justfile = std::fs::read_dir(dir)
      .ok()?
      .flatten()
      .map(|entry| entry.path())
      .find(|path| {
        path.is_file()
          && path.file_name().is_some_and(|n| {
            matches!(
              n.to_string_lossy().to_lowercase().as_str(),
              "justfile" | ".justfile"
            )
          })
      })?;
    let output = Self::just(&justfile)
      .args(["--dump", "--dump-format", "json"])
      .output()
      .ok()?;
    let dump: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let defined =
      |name: &str| dump["recipes"].get(name).is_some() || dump["aliases"].get(name).is_some();
    let recipe = Self::RECIPES.iter().find(|r| defined(r))?;
    Some(Self::Just {
      justfile,
      recipe: (*recipe).to_owned(),
    })
  }

  fn command(&self, dir: &Path, wkit: &Path) -> ProcessBuilder {
    match self {
      Self::Wkit => ProcessBuilder::new(wkit)
        .arg("fmt")
        .cwd(dir)
        .env("WKIT_COLOR", "never"),
      Self::Just { justfile, recipe } => Self::just(justfile).arg(recipe),
    }
  }

  fn label(&self) -> String {
    match self {
      Self::Wkit => "wkit fmt".to_owned(),
      Self::Just { recipe, .. } => format!("just {recipe}"),
    }
  }

  fn just(justfile: &Path) -> ProcessBuilder {
    let dir = justfile.parent().unwrap_or(justfile);
    ProcessBuilder::new("just")
      .arg("--justfile")
      .arg(justfile)
      .arg("--working-directory")
      .arg(dir)
      .cwd(dir)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn wkit_projects_format_with_wkit() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(FmtTarget::detect(dir.path()), None);
    std::fs::write(dir.path().join("wkit.toml"), "").unwrap();
    let target = FmtTarget::detect(dir.path()).unwrap();
    assert_eq!(target.label(), "wkit fmt");
    assert_eq!(
      target
        .command(dir.path(), Path::new("/bin/wkit"))
        .to_string(),
      "/bin/wkit fmt"
    );
  }
}
