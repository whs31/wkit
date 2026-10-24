use wkit_core::Shell;

use crate::{Error, GitRun, Member, Phase, Repo, Result, TaskOutcome, Workspace, WorkspaceLock};

#[derive(Debug, Clone, Copy)]
pub struct Committer<'w> {
  workspace: &'w Workspace,
}

impl<'w> Committer<'w> {
  pub const FORMATTING: &'static str = "CHORE: Formatting";
  pub const LOCK: &'static str = "CHORE: Update lock";

  pub fn new(workspace: &'w Workspace) -> Self {
    Self { workspace }
  }

  pub fn root_changes(&self, paths: &[&str]) -> Vec<String> {
    let mut args = vec!["status", "--porcelain", "-z", "--untracked-files=all", "--"];
    args.extend(paths);
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
    let mut add = vec!["add", "--"];
    add.extend(changed.iter().map(String::as_str));
    let mut commit = vec!["commit", "--quiet", "--message", message, "--"];
    commit.extend(changed.iter().map(String::as_str));
    for args in [add, commit] {
      let run = root.git(&args);
      if !run.success {
        return Err(Error::Message(format!(
          "workspace commit failed\n{}",
          run.output.trim()
        )));
      }
    }
    shell.status(
      "Committed",
      format_args!(
        "workspace {} {}",
        Repo::short(root.head().as_deref()),
        message.lines().next().unwrap_or_default()
      ),
    )?;
    if !push {
      return Ok(());
    }
    let branch = root.current_branch().ok_or_else(|| {
      Error::Message("the workspace is on a detached HEAD; cannot push".to_owned())
    })?;
    let pushed = Self::push(&root, &branch, &Phase::hidden());
    if !pushed.success {
      return Err(Error::Message(format!(
        "workspace push to {branch} failed{}\n{}",
        Self::push_hint(&pushed),
        pushed.output.trim()
      )));
    }
    shell.status("Pushed", format_args!("workspace → origin/{branch}"))?;
    Ok(())
  }

  pub fn lock_changes(&self, lock: &WorkspaceLock) -> Vec<String> {
    let before = self
      .root()
      .query(&["show", &format!("HEAD:{}", WorkspaceLock::FILE)])
      .and_then(|text| WorkspaceLock::parse(&text).ok())
      .unwrap_or_default();
    let mut lines = Vec::new();
    for member in &self.workspace.manifest.members {
      let new = lock.commit(&member.path);
      match before.commit(&member.path) {
        None => lines.push(format!("- {}: added @ {}", member.path, Repo::short(new))),
        Some(old) if Some(old) != new => {
          lines.push(format!(
            "- {}: {} → {}",
            member.path,
            Repo::short(Some(old)),
            Repo::short(new)
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

  pub fn unpublished(&self, lock: &WorkspaceLock, jobs: usize) -> Vec<String> {
    let members = &self.workspace.manifest.members;
    let flags = Workspace::parallel(members, jobs, |member| {
      let repo = self.workspace.repo(member);
      let commit = lock.commit(&member.path).unwrap_or_default();
      repo.is_standalone() && !repo.is_published(commit)
    });
    members
      .iter()
      .zip(flags)
      .filter(|(_, unpublished)| *unpublished)
      .map(|(m, _)| format!("{} @ {}", m.path, Repo::short(lock.commit(&m.path))))
      .collect()
  }

  pub fn commit_lock(&self, shell: &mut Shell, push: bool, jobs: usize) -> Result<()> {
    let lock = self.workspace.lock()?;
    if self.root_changes(&[WorkspaceLock::FILE]).is_empty() {
      shell.status(
        "Fresh",
        format_args!("{} unchanged; nothing to commit", WorkspaceLock::FILE),
      )?;
      return Ok(());
    }
    if push {
      let missing = self.unpublished(&lock, jobs);
      if !missing.is_empty() {
        return Err(Error::Message(format!(
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
    self.commit_root(shell, &message, &[WorkspaceLock::FILE], push)
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
    let committed = repo.git_with_input(
      &[
        "commit",
        "--quiet",
        "--message",
        message,
        "--pathspec-from-file=-",
        "--pathspec-file-nul",
      ],
      &files.join("\0"),
    );
    if !committed.success {
      return TaskOutcome::git_failed("commit failed", &committed);
    }
    let detail = format!(
      "{} ({} files)",
      Repo::short(repo.head().as_deref()),
      files.len()
    );
    if !push {
      return TaskOutcome::done(detail);
    }
    let Some(branch) = repo.current_branch() else {
      return TaskOutcome::failed(format!("{detail}, detached HEAD cannot be pushed"));
    };
    let pushed = Self::push(&repo, &branch, phase);
    if !pushed.success {
      return TaskOutcome::git_failed(
        format!(
          "{detail}, push to {branch} failed{}",
          Self::push_hint(&pushed)
        ),
        &pushed,
      );
    }
    TaskOutcome::done(format!("{detail} → origin/{branch}"))
  }

  fn push(repo: &Repo, branch: &str, phase: &Phase) -> GitRun {
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

  fn root(self) -> Repo {
    Repo::new(&self.workspace.root)
  }
}
