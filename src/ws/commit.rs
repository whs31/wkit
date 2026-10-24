use super::{
  GitRun, LOCK_FILE, Lock, Member, Phase, Repo, TaskOutcome, Workspace, failure, parallel, short,
};
use crate::{Result, Shell};

#[derive(Debug, Clone)]
pub struct Committer<'w> {
  workspace: &'w Workspace,
}

impl<'w> Committer<'w> {
  pub const FORMATTING: &'static str = "CHORE: Formatting";
  const LOCK: &'static str = "CHORE: Update lock";

  pub fn new(workspace: &'w Workspace) -> Self {
    Self { workspace }
  }

  fn root(&self) -> Repo {
    Repo::new(&self.workspace.root)
  }

  pub fn root_changes(&self, paths: &[&str]) -> Vec<String> {
    let args = [
      &["status", "--porcelain", "-z", "--untracked-files=all", "--"],
      paths,
    ]
    .concat();
    let status = self.root().query_raw(&args).unwrap_or_default();
    let changed: Vec<_> = status.split('\0').filter_map(|e| e.get(3..)).collect();
    paths
      .iter()
      .filter(|p| changed.contains(p))
      .map(|p| (*p).to_owned())
      .collect()
  }

  pub fn commit_root(
    &self,
    shell: &mut Shell,
    message: &str,
    paths: &[&str],
    push: bool,
  ) -> Result<()> {
    let changed = self.root_changes(paths);
    if changed.is_empty() {
      shell.status(
        "Fresh",
        format_args!("{}: nothing to commit", paths.join(", ")),
      )?;
      return Ok(());
    }
    let root = self.root();
    let changed: Vec<_> = changed.iter().map(String::as_str).collect();
    for command in [
      &["add", "--"][..],
      &["commit", "--quiet", "--message", message, "--"],
    ] {
      let run = root.git(&[command, &changed].concat());
      if !run.success {
        return Err(failure(format!(
          "workspace commit failed\n{}",
          run.output.trim()
        )));
      }
    }
    let subject = message.lines().next().unwrap_or_default();
    shell.status(
      "Committed",
      format_args!("workspace {} {subject}", short(root.head().as_deref())),
    )?;
    if !push {
      return Ok(());
    }
    let branch = root
      .current_branch()
      .ok_or_else(|| failure("the workspace is on a detached HEAD; cannot push"))?;
    let pushed = push_branch(&root, &branch, &Phase::hidden());
    if !pushed.success {
      return Err(failure(format!(
        "workspace push to {branch} failed{}\n{}",
        push_hint(&pushed),
        pushed.output.trim()
      )));
    }
    shell.status("Pushed", format_args!("workspace → origin/{branch}"))?;
    Ok(())
  }

  fn lock_changes(&self, lock: &Lock) -> Vec<String> {
    let before = self
      .root()
      .query(&["show", &format!("HEAD:{LOCK_FILE}")])
      .and_then(|text| Lock::parse(&text).ok())
      .unwrap_or_default();
    let mut lines = Vec::new();
    for member in &self.workspace.manifest.members {
      let new = lock.commit(&member.path);
      match before.commit(&member.path) {
        None => lines.push(format!("- {}: added @ {}", member.path, short(new))),
        Some(old) if Some(old) != new => {
          lines.push(format!(
            "- {}: {} → {}",
            member.path,
            short(Some(old)),
            short(new)
          ));
        }
        Some(_) => {}
      }
    }
    lines.extend(
      before
        .revisions
        .keys()
        .filter(|p| lock.commit(p).is_none())
        .map(|p| format!("- {p}: removed")),
    );
    lines
  }

  fn unpublished(&self, lock: &Lock, jobs: usize) -> Vec<String> {
    let members = &self.workspace.manifest.members;
    let flags = parallel(members, jobs, |member| {
      let repo = self.workspace.repo(member);
      repo.is_standalone() && !repo.is_published(lock.commit(&member.path).unwrap_or_default())
    });
    members
      .iter()
      .zip(flags)
      .filter(|(_, unpublished)| *unpublished)
      .map(|(m, _)| format!("{} @ {}", m.path, short(lock.commit(&m.path))))
      .collect()
  }

  pub fn commit_lock(&self, shell: &mut Shell, push: bool, jobs: usize) -> Result<()> {
    let lock = self.workspace.lock()?;
    if self.root_changes(&[LOCK_FILE]).is_empty() {
      shell.status(
        "Fresh",
        format_args!("{LOCK_FILE} unchanged; nothing to commit"),
      )?;
      return Ok(());
    }
    if push {
      let missing = self.unpublished(&lock, jobs);
      if !missing.is_empty() {
        return Err(failure(format!(
          "refusing to push a lock that references unpublished commits:\n  {}\npush those project commits first",
          missing.join("\n  ")
        )));
      }
    }
    let changes = self.lock_changes(&lock);
    let message = if changes.is_empty() {
      Self::LOCK.to_owned()
    } else {
      format!("{}\n\n{}", Self::LOCK, changes.join("\n"))
    };
    self.commit_root(shell, &message, &[LOCK_FILE], push)
  }

  pub fn commit_member(
    &self,
    member: &Member,
    message: &str,
    push: bool,
    phase: &Phase,
  ) -> TaskOutcome {
    let repo = self.workspace.repo(member);
    let files = repo.changed_files();
    if files.is_empty() {
      return TaskOutcome::fresh("no changes");
    }
    let paths: String = files.iter().flat_map(|f| [f.as_str(), "\0"]).collect();
    let staged = repo.git_with_input(&["update-index", "-z", "--remove", "--stdin"], &paths);
    if !staged.success {
      return TaskOutcome::git_failed("staging failed", &staged);
    }
    let committed = repo.git(&["commit", "--quiet", "--message", message]);
    if !committed.success {
      return TaskOutcome::git_failed("commit failed", &committed);
    }
    let detail = format!("{} ({} files)", short(repo.head().as_deref()), files.len());
    if !push {
      return TaskOutcome::done(detail);
    }
    let Some(branch) = repo.current_branch() else {
      return TaskOutcome::failed(format!("{detail}, detached HEAD cannot be pushed"));
    };
    let pushed = push_branch(&repo, &branch, phase);
    if !pushed.success {
      return TaskOutcome::git_failed(
        format!("{detail}, push to {branch} failed{}", push_hint(&pushed)),
        &pushed,
      );
    }
    TaskOutcome::done(format!("{detail} → origin/{branch}"))
  }
}

fn push_branch(repo: &Repo, branch: &str, phase: &Phase) -> GitRun {
  repo.with_progress(
    &[
      "push",
      "--progress",
      "origin",
      &format!("HEAD:refs/heads/{branch}"),
    ],
    phase,
    "pushing",
  )
}

fn push_hint(run: &GitRun) -> &'static str {
  if run.output.contains("rejected") {
    " (remote moved? run `wkit ws update` first)"
  } else {
    ""
  }
}
