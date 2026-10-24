use wkit_core::Shell;

use crate::{
  Error, Member, MemberStatus, MemberSync, Phase, Repo, Report, Result, Runner, TaskOutcome,
  WorkspaceLock, WorkspaceManifest,
};

#[derive(Debug, Clone)]
pub struct Workspace {
  pub root: std::path::PathBuf,
  pub manifest: WorkspaceManifest,
}

impl Workspace {
  pub const GITIGNORE: &str = ".gitignore";

  pub fn discover(start: &std::path::Path) -> Result<Self> {
    let path = wkit_core::fs::find_upwards(start, WorkspaceManifest::FILE).ok_or_else(|| {
      Error::NotFound {
        start: start.to_owned(),
      }
    })?;
    Self::load(path.parent().unwrap_or(start))
  }

  pub fn load(root: &std::path::Path) -> Result<Self> {
    Ok(Self {
      root: root.to_owned(),
      manifest: WorkspaceManifest::load(&root.join(WorkspaceManifest::FILE))?,
    })
  }

  pub fn manifest_path(&self) -> std::path::PathBuf {
    self.root.join(WorkspaceManifest::FILE)
  }

  pub fn lock_path(&self) -> std::path::PathBuf {
    self.root.join(WorkspaceLock::FILE)
  }

  pub fn lock(&self) -> Result<WorkspaceLock> {
    let lock = WorkspaceLock::load(&self.lock_path())?;
    lock.check(&self.manifest)?;
    Ok(lock)
  }

  pub fn repo(&self, member: &Member) -> Repo {
    Repo::new(member.dir(&self.root))
  }

  pub fn init(&self, shell: &mut Shell, runner: Runner) -> Result<Report> {
    let lock = if self.lock_path().is_file() {
      WorkspaceLock::load(&self.lock_path())?
    } else {
      WorkspaceLock::default()
    };
    runner.run(
      shell,
      "Cloning",
      "Cloned",
      &self.manifest.members,
      |member, phase| {
        let sync = MemberSync::new(&self.root, member, phase);
        match lock.commit(&member.path) {
          Some(commit) => sync.init(commit),
          None if self.repo(member).exists() => sync.init(""),
          None => match Repo::remote_branch_head(&member.url, &member.branch) {
            Some(head) => sync.clone_locked(&head),
            None => TaskOutcome::failed(format!(
              "branch `{}` not found on {}",
              member.branch, member.url
            )),
          },
        }
      },
    )
  }

  pub fn is_fully_locked(&self) -> bool {
    WorkspaceLock::load(&self.lock_path()).is_ok_and(|lock| lock.check(&self.manifest).is_ok())
  }

  pub fn restore(&self, shell: &mut Shell, runner: Runner) -> Result<Report> {
    let lock = self.lock()?;
    runner.run(
      shell,
      "Restoring",
      "Restored",
      &self.manifest.members,
      |member, phase| {
        MemberSync::new(&self.root, member, phase)
          .restore(lock.commit(&member.path).unwrap_or_default())
      },
    )
  }

  pub fn update(&self, shell: &mut Shell, runner: Runner) -> Result<Report> {
    runner.run(
      shell,
      "Updating",
      "Updated",
      &self.manifest.members,
      |member, phase| MemberSync::new(&self.root, member, phase).update(),
    )
  }

  pub fn relock(&self, shell: &mut Shell, jobs: usize) -> Result<usize> {
    let states = Self::parallel(&self.manifest.members, jobs, |member| {
      if let Some(problem) = MemberSync::new(&self.root, member, &Phase::hidden()).validate() {
        return Err(problem.detail);
      }
      let repo = self.repo(member);
      if repo.is_dirty() {
        return Err("uncommitted changes".to_owned());
      }
      let head = repo
        .head()
        .ok_or_else(|| "cannot resolve HEAD".to_owned())?;
      let published = repo.is_published(&head);
      Ok((head, published))
    });
    let failures: Vec<_> = self
      .manifest
      .members
      .iter()
      .zip(&states)
      .filter_map(|(m, s)| s.as_ref().err().map(|e| format!("  {}: {e}", m.path)))
      .collect();
    if !failures.is_empty() {
      return Err(Error::Message(format!(
        "{} not changed:\n{}",
        WorkspaceLock::FILE,
        failures.join("\n")
      )));
    }
    let previous = WorkspaceLock::load(&self.lock_path()).unwrap_or_default();
    let mut lock = WorkspaceLock::default();
    let mut changed = 0;
    for (member, state) in self.manifest.members.iter().zip(states) {
      let (head, published) = state.expect("failures returned above");
      let old = previous.commit(&member.path);
      if old != Some(head.as_str()) {
        changed += 1;
        shell.status(
          "Locking",
          format_args!(
            "{}  {} → {}",
            member.path,
            Repo::short(old),
            Repo::short(Some(&head))
          ),
        )?;
      }
      if !published {
        shell.warn(format_args!(
          "{}: {} is not on any origin branch (push it)",
          member.path,
          Repo::short(Some(&head))
        ))?;
      }
      lock.revisions.insert(member.path.clone(), head);
    }
    Self::write_atomic(&self.lock_path(), &lock.render(&self.manifest)?)?;
    shell.status(
      "Locked",
      format_args!(
        "{changed} of {} projects changed in {}",
        self.manifest.members.len(),
        WorkspaceLock::FILE
      ),
    )?;
    Ok(changed)
  }

  pub fn status(&self, jobs: usize) -> Result<Vec<MemberStatus>> {
    let lock = WorkspaceLock::load(&self.lock_path()).unwrap_or_default();
    Ok(Self::parallel(&self.manifest.members, jobs, |member| {
      MemberStatus::inspect(&self.root, member, lock.commit(&member.path))
    }))
  }

  pub fn add(
    &mut self,
    shell: &mut Shell,
    url: &str,
    path: Option<&str>,
    branch: Option<&str>,
  ) -> Result<Member> {
    let lock = self.lock()?;
    if self.manifest.members.iter().any(|m| m.url == url) {
      return Err(Error::config(
        WorkspaceManifest::FILE,
        format!("{url} is already listed"),
      ));
    }
    let branch = match branch {
      Some(branch) => branch.to_owned(),
      None => Repo::remote_default_branch(url).ok_or_else(|| {
        Error::Message(format!(
          "cannot determine the default branch of {url} (unreachable? pass --branch)"
        ))
      })?,
    };
    if !Member::is_valid_branch(&branch) {
      return Err(Error::Message(format!("invalid branch `{branch}`")));
    }
    let path = path.map_or_else(
      || self.manifest.derive_path(url),
      |p| p.trim_matches('/').to_owned(),
    );
    if !Member::is_valid_path(&path) {
      return Err(Error::Message(format!("invalid project path `{path}`")));
    }
    let member = Member::new(&path, url, &branch);
    let text = WorkspaceManifest::read(&self.manifest_path())?;
    let new_text = self.manifest.with_member(&text, &member);
    let manifest = WorkspaceManifest::parse(&new_text)?;

    let repo = self.repo(&member);
    let head = if repo.exists() {
      if let Some(problem) = MemberSync::new(&self.root, &member, &Phase::hidden()).validate() {
        return Err(Error::Message(format!(
          "{path} already exists: {}",
          problem.detail
        )));
      }
      let head = repo
        .head()
        .ok_or_else(|| Error::Message(format!("{path}: cannot resolve HEAD")))?;
      shell.status(
        "Adopting",
        format_args!("{path}  existing clone @ {}", Repo::short(Some(&head))),
      )?;
      head
    } else {
      let head = Repo::remote_branch_head(url, &branch)
        .ok_or_else(|| Error::Message(format!("branch `{branch}` not found on {url}")))?;
      let report = Runner::new(1).run(
        shell,
        "Cloning",
        "Cloned",
        std::slice::from_ref(&member),
        |m, phase| MemberSync::new(&self.root, m, phase).clone_locked(&head),
      )?;
      if let Some((_, outcome)) = report.failures().next() {
        return Err(Error::Message(format!(
          "{path}: {}\n{}",
          outcome.detail,
          outcome.output.trim()
        )));
      }
      head
    };

    let mut lock = lock;
    lock.revisions.insert(path.clone(), head.clone());
    let rendered = lock.render(&manifest)?;
    Self::write_atomic(&self.manifest_path(), &new_text)?;
    Self::write_atomic(&self.lock_path(), &rendered)?;
    let ignored = self.ensure_ignored(&path)?;
    self.manifest = manifest;
    shell.status(
      "Added",
      format_args!(
        "{path} ({branch} @ {}) to {} and {}{}",
        Repo::short(Some(&head)),
        WorkspaceManifest::FILE,
        WorkspaceLock::FILE,
        if ignored {
          format!("; updated {}", Self::GITIGNORE)
        } else {
          String::new()
        }
      ),
    )?;
    Ok(member)
  }

  pub fn ensure_ignored(&self, path: &str) -> Result<bool> {
    let top = path.split('/').next().unwrap_or(path);
    let pattern = format!("/{top}/");
    let file = self.root.join(Self::GITIGNORE);
    let text = wkit_core::fs::read_optional(&file)?.unwrap_or_default();
    if text.lines().any(|l| l.trim() == pattern) {
      return Ok(false);
    }
    let mut anchored: Vec<_> = text
      .lines()
      .filter(|l| l.starts_with('/'))
      .map(str::trim)
      .filter(|l| !l.is_empty())
      .map(ToOwned::to_owned)
      .chain([pattern])
      .collect();
    anchored.sort();
    anchored.dedup();
    let rest = text
      .lines()
      .filter(|l| !l.trim().is_empty() && !l.starts_with('/'));
    let lines: Vec<_> = anchored.iter().map(String::as_str).chain(rest).collect();
    Self::write_atomic(&file, &format!("{}\n", lines.join("\n")))?;
    Ok(true)
  }

  pub fn cloned(&self) -> Vec<&Member> {
    self
      .manifest
      .members
      .iter()
      .filter(|m| m.dir(&self.root).join(".git").exists())
      .collect()
  }

  pub fn parallel<T, F>(members: &[Member], jobs: usize, f: F) -> Vec<T>
  where
    T: Send,
    F: Fn(&Member) -> T + Sync,
  {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results: std::sync::Mutex<Vec<Option<T>>> =
      std::sync::Mutex::new((0..members.len()).map(|_| None).collect());
    std::thread::scope(|scope| {
      for _ in 0..jobs.clamp(1, members.len().max(1)) {
        scope.spawn(|| {
          loop {
            let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let Some(member) = members.get(index) else {
              break;
            };
            let value = f(member);
            results.lock().expect("not poisoned")[index] = Some(value);
          }
        });
      }
    });
    results
      .into_inner()
      .expect("not poisoned")
      .into_iter()
      .map(|v| v.expect("every member ran"))
      .collect()
  }

  pub fn write_atomic(path: &std::path::Path, text: &str) -> Result<()> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    wkit_core::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, path).map_err(|source| {
      let _ = std::fs::remove_file(&temporary);
      wkit_core::Error::Write {
        path: path.to_owned(),
        source,
      }
    })?;
    Ok(())
  }
}
