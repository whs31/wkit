use std::path::Path;

use super::{GitRun, Member, Phase, Repo, TaskOutcome, short};

#[derive(Debug)]
pub struct MemberSync<'a> {
  member: &'a Member,
  repo: Repo,
  phase: &'a Phase,
}

impl<'a> MemberSync<'a> {
  pub fn new(root: &Path, member: &'a Member, phase: &'a Phase) -> Self {
    Self {
      member,
      repo: Repo::new(member.dir(root)),
      phase,
    }
  }

  pub fn validate(&self) -> Option<TaskOutcome> {
    let problem = if !self.repo.exists() {
      "missing (run `wkit ws init`)".to_owned()
    } else if !self.repo.is_repository() {
      "path exists but is not a Git repository".to_owned()
    } else if !self.repo.is_standalone() {
      "not a standalone clone (.git is not a directory)".to_owned()
    } else {
      match self.repo.origin_url() {
        Some(url) if url == self.member.url => return None,
        url => format!("origin mismatch: {}", url.as_deref().unwrap_or("missing")),
      }
    };
    Some(TaskOutcome::failed(problem))
  }

  pub fn init(&self, commit: &str) -> TaskOutcome {
    if !self.repo.exists() {
      return self.clone_locked(commit);
    }
    self.validate().unwrap_or_else(|| {
      TaskOutcome::fresh(format!(
        "already present @ {}",
        short(self.repo.head().as_deref())
      ))
    })
  }

  fn fetch(&self) -> Result<(), TaskOutcome> {
    let fetch = self
      .repo
      .with_progress(&["fetch", "--progress", "origin"], self.phase, "fetching");
    if fetch.success {
      Ok(())
    } else {
      Err(TaskOutcome::git_failed("fetch failed", &fetch))
    }
  }

  pub fn restore(&self, commit: &str) -> TaskOutcome {
    if let Some(problem) = self.validate() {
      return problem;
    }
    if let Err(failed) = self.fetch() {
      return failed;
    }
    self.checkout_locked(&self.repo, commit, false)
  }

  pub fn update(&self) -> TaskOutcome {
    self.try_update().unwrap_or_else(|outcome| outcome)
  }

  fn try_update(&self) -> Result<TaskOutcome, TaskOutcome> {
    if let Some(problem) = self.validate() {
      return Err(problem);
    }
    let repo = &self.repo;
    if repo.is_dirty() {
      return Err(TaskOutcome::failed("uncommitted changes; update refused"));
    }
    let before = repo.head();
    self.fetch()?;
    let wanted = &self.member.branch;
    let branch = match repo.current_branch() {
      Some(branch) => branch,
      None
        if repo
          .revision(&format!("refs/remotes/origin/{wanted}"))
          .is_none() =>
      {
        return Err(TaskOutcome::failed(format!(
          "origin/{wanted} not found (fix the branch in workspace.toml)"
        )));
      }
      None => {
        let switch = self.switch_to_branch(repo, &[]);
        if !switch.success {
          return Err(TaskOutcome::git_failed(
            format!("cannot switch to {wanted}"),
            &switch,
          ));
        }
        wanted.clone()
      }
    };
    let mut tracking = repo.upstream();
    if tracking.is_none()
      && branch == *wanted
      && repo.revision(&format!("origin/{branch}")).is_some()
      && repo
        .git(&[
          "branch",
          &format!("--set-upstream-to=origin/{branch}"),
          &branch,
        ])
        .success
    {
      tracking = Some(format!("origin/{branch}"));
    }
    let tracking = tracking
      .ok_or_else(|| TaskOutcome::warning(format!("{branch}: no upstream, not updated")))?;
    let (Some(head), Some(target)) = (repo.head(), repo.revision(&tracking)) else {
      return Err(TaskOutcome::failed(format!(
        "cannot resolve HEAD or {tracking}"
      )));
    };
    if head != target {
      if repo.is_ancestor(&head, &target) {
        self.phase.set("fast-forwarding", None);
        let merge = repo.git(&["merge", "--quiet", "--ff-only", &tracking]);
        if !merge.success {
          return Err(TaskOutcome::git_failed(
            format!("{branch}: fast-forward failed"),
            &merge,
          ));
        }
      } else if repo.is_ancestor(&target, &head) {
        let ahead = repo
          .query(&["rev-list", "--count", &format!("{tracking}..HEAD")])
          .unwrap_or_else(|| "?".to_owned());
        return Err(TaskOutcome::warning(format!(
          "{branch}: {ahead} commit(s) ahead of {tracking}"
        )));
      } else {
        return Err(TaskOutcome::failed(format!(
          "{branch}: diverged from {tracking}; merge or rebase manually"
        )));
      }
    }
    let warning = self.update_submodules(repo);
    let after = repo.head();
    let outcome = if before == after && repo.current_branch().as_deref() == Some(&branch) {
      TaskOutcome::fresh(format!("{branch} up to date @ {}", short(after.as_deref())))
    } else {
      TaskOutcome::done(format!(
        "{branch} {} → {}",
        short(before.as_deref()),
        short(after.as_deref())
      ))
    };
    Ok(outcome.with_warning(warning))
  }

  pub fn clone_locked(&self, commit: &str) -> TaskOutcome {
    let destination = self.repo.dir();
    let parent = destination.parent().unwrap_or(destination);
    let staging = parent.join(format!(
      ".{}.partial",
      destination
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
    ));
    let _ = std::fs::remove_dir_all(&staging);
    let outcome = self.clone_into(parent, &staging, commit);
    if outcome.is_failed() {
      let _ = std::fs::remove_dir_all(&staging);
      return outcome;
    }
    let branch = &self.member.branch;
    let warning = self
      .repo
      .revision(&format!("refs/remotes/origin/{branch}"))
      .is_none()
      .then(|| format!("origin has no branch `{branch}` (fix workspace.toml)"));
    outcome.with_warning(warning)
  }

  fn clone_into(&self, parent: &Path, staging: &Path, commit: &str) -> TaskOutcome {
    if let Err(e) = std::fs::create_dir_all(parent) {
      return TaskOutcome::failed(format!("cannot create `{}`: {e}", parent.display()));
    }
    let staging_arg = staging.to_string_lossy();
    let args = [
      "clone",
      "--progress",
      "--no-checkout",
      "--",
      &self.member.url,
      &staging_arg,
    ];
    let clone = Repo::new(parent).with_progress(&args, self.phase, "cloning");
    if !clone.success {
      return TaskOutcome::git_failed("clone failed", &clone);
    }
    let outcome = self.checkout_locked(&Repo::new(staging), commit, true);
    if outcome.is_failed() {
      return outcome;
    }
    match std::fs::rename(staging, self.repo.dir()) {
      Ok(()) => outcome,
      Err(e) => TaskOutcome::failed(format!("cannot move the clone into place: {e}")),
    }
  }

  fn checkout_locked(&self, repo: &Repo, commit: &str, fresh: bool) -> TaskOutcome {
    if !fresh && repo.is_dirty() {
      return TaskOutcome::failed("uncommitted changes; lock restore refused");
    }
    if repo.revision(commit).is_none() {
      let fetch = repo.with_progress(
        &["fetch", "--progress", "origin", commit],
        self.phase,
        "fetching locked commit",
      );
      if !fetch.success {
        return TaskOutcome::git_failed(
          format!("cannot fetch locked commit {}", short(Some(commit))),
          &fetch,
        );
      }
    }
    let before = if fresh { None } else { repo.head() };
    if before.as_deref() != Some(commit) {
      self.phase.set("checking out", None);
      let force: &[&str] = if fresh { &["--discard-changes"] } else { &[] };
      let branch = &self.member.branch;
      let target = repo.revision(&format!("refs/remotes/origin/{branch}"));
      let local = repo.revision(&format!("refs/heads/{branch}"));
      let switch =
        if target.as_deref() == Some(commit) && local.as_deref().is_none_or(|l| l == commit) {
          self.switch_to_branch(repo, force)
        } else {
          repo.git(&[&["switch", "--quiet"], force, &["--detach", commit]].concat())
        };
      if !switch.success {
        return TaskOutcome::git_failed(
          format!("cannot check out locked commit {}", short(Some(commit))),
          &switch,
        );
      }
    }
    let warning = self.update_submodules(repo);
    let label = repo
      .current_branch()
      .unwrap_or_else(|| "detached".to_owned());
    let outcome = if fresh {
      TaskOutcome::done(format!("{label} @ {}", short(Some(commit))))
    } else if before.as_deref() == Some(commit) {
      TaskOutcome::fresh(format!("{label} @ {}", short(Some(commit))))
    } else {
      TaskOutcome::done(format!(
        "{label} {} → {}",
        short(before.as_deref()),
        short(Some(commit))
      ))
    };
    outcome.with_warning(warning)
  }

  fn switch_to_branch(&self, repo: &Repo, force: &[&str]) -> GitRun {
    let branch = self.member.branch.as_str();
    let upstream = format!("origin/{branch}");
    let target: &[&str] = if repo.revision(&format!("refs/heads/{branch}")).is_none() {
      &["--create", branch, "--track", &upstream]
    } else {
      &[branch]
    };
    repo.git(&[&["switch", "--quiet"], force, target].concat())
  }

  fn update_submodules(&self, repo: &Repo) -> Option<String> {
    if !repo.dir().join(".gitmodules").is_file() {
      return None;
    }
    let args = ["submodule", "update", "--init", "--recursive", "--progress"];
    let nested = repo.with_progress(&args, self.phase, "submodules");
    (!nested.success).then(|| format!("nested submodules: {}", nested.fatal().unwrap_or("failed")))
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberStatus {
  pub path: String,
  pub head: Option<String>,
  pub branch: Option<String>,
  pub flags: Vec<String>,
  pub broken: bool,
}

impl MemberStatus {
  pub fn inspect(root: &Path, member: &Member, locked: Option<&str>) -> Self {
    let repo = Repo::new(member.dir(root));
    let mut status = Self {
      path: member.path.clone(),
      head: None,
      branch: None,
      flags: Vec::new(),
      broken: true,
    };
    if !repo.exists() {
      status.flags.push("missing".to_owned());
      return status;
    }
    if !repo.is_repository() {
      status.flags.push("not a Git repository".to_owned());
      return status;
    }
    status.broken = !repo.is_standalone();
    if status.broken {
      status.flags.push("not standalone".to_owned());
    }
    status.head = repo.head();
    status.branch = repo.current_branch();
    if repo.is_dirty() {
      status.flags.push("dirty".to_owned());
    }
    if status.head.as_deref() != locked {
      status.flags.push("unlocked".to_owned());
    }
    if let Some((ahead, behind)) = repo.upstream().and_then(|u| repo.ahead_behind(&u)) {
      status.flags.extend(
        [("ahead", ahead), ("behind", behind)]
          .iter()
          .filter(|(_, n)| *n > 0)
          .map(|(w, n)| format!("{w} {n}")),
      );
    }
    status
  }

  pub fn render(&self, branch_width: usize) -> String {
    let flags = if self.flags.is_empty() {
      String::new()
    } else {
      format!("  {}", self.flags.join(", "))
    };
    let branch = if self.head.is_none() && self.broken {
      ""
    } else {
      self.branch.as_deref().unwrap_or("detached")
    };
    format!(
      "{:<8}  {branch:<branch_width$}  {}{flags}",
      short(self.head.as_deref()),
      self.path
    )
  }
}
