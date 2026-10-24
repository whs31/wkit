use crate::{Member, Phase, Repo, TaskOutcome};

#[derive(Debug)]
pub struct MemberSync<'a> {
  member: &'a Member,
  repo: Repo,
  phase: &'a Phase,
}

impl<'a> MemberSync<'a> {
  pub fn new(root: &std::path::Path, member: &'a Member, phase: &'a Phase) -> Self {
    Self {
      member,
      repo: Repo::new(member.dir(root)),
      phase,
    }
  }

  pub fn validate(&self) -> Option<TaskOutcome> {
    if !self.repo.exists() {
      return Some(TaskOutcome::failed("missing (run `wkit ws init`)"));
    }
    if !self.repo.is_repository() {
      return Some(TaskOutcome::failed(
        "path exists but is not a Git repository",
      ));
    }
    if !self.repo.is_standalone() {
      return Some(TaskOutcome::failed(
        "not a standalone clone (.git is not a directory)",
      ));
    }
    match self.repo.origin_url() {
      Some(url) if url == self.member.url => None,
      url => Some(TaskOutcome::failed(format!(
        "origin mismatch: {}",
        url.as_deref().unwrap_or("missing")
      ))),
    }
  }

  pub fn init(&self, commit: &str) -> TaskOutcome {
    if self.repo.exists() {
      return self.validate().unwrap_or_else(|| {
        TaskOutcome::fresh(format!(
          "already present @ {}",
          Repo::short(self.repo.head().as_deref())
        ))
      });
    }
    self.clone_locked(commit)
  }

  pub fn restore(&self, commit: &str) -> TaskOutcome {
    if let Some(problem) = self.validate() {
      return problem;
    }
    let fetch = self
      .repo
      .with_progress(&["fetch", "--progress", "origin"], self.phase, "fetching");
    if !fetch.success {
      return TaskOutcome::git_failed("fetch failed", &fetch);
    }
    self.checkout_locked(&self.repo, commit, false)
  }

  pub fn update(&self) -> TaskOutcome {
    if let Some(problem) = self.validate() {
      return problem;
    }
    let repo = &self.repo;
    if repo.is_dirty() {
      return TaskOutcome::failed("uncommitted changes; update refused");
    }
    let before = repo.head();
    let fetch = repo.with_progress(&["fetch", "--progress", "origin"], self.phase, "fetching");
    if !fetch.success {
      return TaskOutcome::git_failed("fetch failed", &fetch);
    }
    let branch = if let Some(branch) = repo.current_branch() {
      branch
    } else {
      let wanted = &self.member.branch;
      if repo
        .revision(&format!("refs/remotes/origin/{wanted}"))
        .is_none()
      {
        return TaskOutcome::failed(format!(
          "origin/{wanted} not found (fix the branch in workspace.toml)"
        ));
      }
      let switch = self.switch_to_branch(repo, &[]);
      if !switch.success {
        return TaskOutcome::git_failed(format!("cannot switch to {wanted}"), &switch);
      }
      wanted.clone()
    };
    let mut tracking = repo.upstream();
    if tracking.is_none()
      && branch == self.member.branch
      && repo.revision(&format!("origin/{branch}")).is_some()
      && repo
        .git([
          "branch",
          &format!("--set-upstream-to=origin/{branch}"),
          &branch,
        ])
        .success
    {
      tracking = Some(format!("origin/{branch}"));
    }
    let Some(tracking) = tracking else {
      return TaskOutcome::warning(format!("{branch}: no upstream, not updated"));
    };
    let (Some(head), Some(target)) = (repo.revision("HEAD"), repo.revision(&tracking)) else {
      return TaskOutcome::failed(format!("cannot resolve HEAD or {tracking}"));
    };
    if head != target {
      if repo.is_ancestor(&head, &target) {
        self.phase.set("fast-forwarding", None);
        let merge = repo.git(["merge", "--quiet", "--ff-only", &tracking]);
        if !merge.success {
          return TaskOutcome::git_failed(format!("{branch}: fast-forward failed"), &merge);
        }
      } else if repo.is_ancestor(&target, &head) {
        let ahead = repo
          .query(&["rev-list", "--count", &format!("{tracking}..HEAD")])
          .unwrap_or_else(|| "?".to_owned());
        return TaskOutcome::warning(format!("{branch}: {ahead} commit(s) ahead of {tracking}"));
      } else {
        return TaskOutcome::failed(format!(
          "{branch}: diverged from {tracking}; merge or rebase manually"
        ));
      }
    }
    let warning = self.update_submodules(repo);
    let after = repo.head();
    let outcome = if before == after && repo.current_branch().as_deref() == Some(&branch) {
      TaskOutcome::fresh(format!(
        "{branch} up to date @ {}",
        Repo::short(after.as_deref())
      ))
    } else {
      TaskOutcome::done(format!(
        "{branch} {} → {}",
        Repo::short(before.as_deref()),
        Repo::short(after.as_deref())
      ))
    };
    outcome.with_warning(warning)
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
    if let Err(e) = std::fs::create_dir_all(parent) {
      return TaskOutcome::failed(format!("cannot create `{}`: {e}", parent.display()));
    }
    let staging_arg = staging.to_string_lossy();
    let clone = Repo::new(parent).with_progress(
      &[
        "clone",
        "--progress",
        "--no-checkout",
        "--",
        &self.member.url,
        &staging_arg,
      ],
      self.phase,
      "cloning",
    );
    if !clone.success {
      let _ = std::fs::remove_dir_all(&staging);
      return TaskOutcome::git_failed("clone failed", &clone);
    }
    let staged = Repo::new(&staging);
    let outcome = self.checkout_locked(&staged, commit, true);
    if outcome.is_failed() {
      let _ = std::fs::remove_dir_all(&staging);
      return outcome;
    }
    if let Err(e) = std::fs::rename(&staging, destination) {
      let _ = std::fs::remove_dir_all(&staging);
      return TaskOutcome::failed(format!("cannot move the clone into place: {e}"));
    }
    let warning = self
      .repo
      .revision(&format!("refs/remotes/origin/{}", self.member.branch))
      .is_none()
      .then(|| {
        format!(
          "origin has no branch `{}` (fix workspace.toml)",
          self.member.branch
        )
      });
    outcome.with_warning(warning)
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
          format!("cannot fetch locked commit {}", Repo::short(Some(commit))),
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
          let mut args = vec!["switch", "--quiet"];
          args.extend(force);
          args.extend(["--detach", commit]);
          repo.git(&args)
        };
      if !switch.success {
        return TaskOutcome::git_failed(
          format!(
            "cannot check out locked commit {}",
            Repo::short(Some(commit))
          ),
          &switch,
        );
      }
    }
    let warning = self.update_submodules(repo);
    let label = repo
      .current_branch()
      .unwrap_or_else(|| "detached".to_owned());
    let outcome = if fresh {
      TaskOutcome::done(format!("{label} @ {}", Repo::short(Some(commit))))
    } else if before.as_deref() == Some(commit) {
      TaskOutcome::fresh(format!("{label} @ {}", Repo::short(Some(commit))))
    } else {
      TaskOutcome::done(format!(
        "{label} {} → {}",
        Repo::short(before.as_deref()),
        Repo::short(Some(commit))
      ))
    };
    outcome.with_warning(warning)
  }

  fn switch_to_branch(&self, repo: &Repo, force: &[&str]) -> crate::GitRun {
    let branch = &self.member.branch;
    let mut args = vec!["switch", "--quiet"];
    args.extend(force);
    let upstream = format!("origin/{branch}");
    if repo.revision(&format!("refs/heads/{branch}")).is_none() {
      args.extend(["--create", branch, "--track", &upstream]);
    } else {
      args.push(branch);
    }
    repo.git(&args)
  }

  fn update_submodules(&self, repo: &Repo) -> Option<String> {
    if !repo.dir().join(".gitmodules").is_file() {
      return None;
    }
    let nested = repo.with_progress(
      &["submodule", "update", "--init", "--recursive", "--progress"],
      self.phase,
      "submodules",
    );
    (!nested.success).then(|| format!("nested submodules: {}", nested.fatal().unwrap_or("failed")))
  }
}
