use std::fmt::Write as _;

use wkit_core::Shell;
use wkit_ws::{Committer, Repo, Runner, TaskStatus, Workspace, WorkspaceLock, WorkspaceManifest};

fn git(dir: &std::path::Path, args: &[&str]) {
  let status = std::process::Command::new("git")
    .current_dir(dir)
    .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
    .args(args)
    .output()
    .unwrap();
  assert!(
    status.status.success(),
    "git {args:?}: {}",
    String::from_utf8_lossy(&status.stderr)
  );
}

struct Fixture {
  dir: tempfile::TempDir,
}

impl Fixture {
  fn new(remotes: &[(&str, &str)]) -> Self {
    let fixture = Self {
      dir: tempfile::tempdir().unwrap(),
    };
    let mut manifest = "version = 1\n".to_owned();
    for (path, branch) in remotes {
      fixture.remote(path, branch);
      let _ = write!(
        manifest,
        "\n[[project]]\npath = \"{path}\"\nurl = \"{}\"\nbranch = \"{branch}\"\n",
        fixture.url(path)
      );
    }
    let root = fixture.root();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(WorkspaceManifest::FILE), manifest).unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "workspace"]);
    fixture
  }

  fn root(&self) -> std::path::PathBuf {
    self.dir.path().join("ws")
  }

  fn url(&self, path: &str) -> String {
    self
      .dir
      .path()
      .join("remotes")
      .join(format!("{path}.git"))
      .display()
      .to_string()
  }

  fn seed(&self, path: &str) -> std::path::PathBuf {
    self.dir.path().join("seed").join(path)
  }

  fn remote(&self, path: &str, branch: &str) {
    let url = self.url(path);
    std::fs::create_dir_all(&url).unwrap();
    git(
      std::path::Path::new(&url),
      &["init", "-q", "--bare", "-b", branch],
    );
    let seed = self.seed(path);
    std::fs::create_dir_all(&seed).unwrap();
    git(&seed, &["init", "-q", "-b", branch]);
    git(&seed, &["remote", "add", "origin", &url]);
    self.push_commit(path, "first");
  }

  fn push_commit(&self, path: &str, text: &str) -> String {
    let seed = self.seed(path);
    std::fs::write(seed.join("README"), text).unwrap();
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "-qm", text]);
    git(&seed, &["push", "-q", "origin", "HEAD"]);
    Repo::new(&seed).head().unwrap()
  }

  fn workspace(&self) -> Workspace {
    let nested = self.root().join("docs/notes");
    std::fs::create_dir_all(&nested).unwrap();
    Workspace::discover(&nested).unwrap()
  }
}

fn shell() -> Shell {
  Shell::from_write(Box::new(std::io::sink()))
}

fn statuses(report: &wkit_ws::Report) -> Vec<(&str, TaskStatus)> {
  report
    .outcomes
    .iter()
    .map(|(p, o)| (p.as_str(), o.status))
    .collect()
}

#[test]
fn init_clones_at_the_branch_tip_and_lock_records_it() {
  let fixture = Fixture::new(&[("support/core", "main"), ("protocol/link", "develop")]);
  let workspace = fixture.workspace();
  let mut shell = shell();
  assert!(!workspace.is_fully_locked());
  let report = workspace.init(&mut shell, Runner::new(4)).unwrap();
  assert_eq!(
    statuses(&report),
    [
      ("support/core", TaskStatus::Done),
      ("protocol/link", TaskStatus::Done)
    ]
  );
  assert_eq!(workspace.relock(&mut shell, 4).unwrap(), 2);
  let lock = workspace.lock().unwrap();
  let core = Repo::new(fixture.seed("support/core")).head().unwrap();
  assert_eq!(lock.commit("support/core"), Some(core.as_str()));
  assert_eq!(
    Repo::new(fixture.root().join("protocol/link"))
      .current_branch()
      .as_deref(),
    Some("develop")
  );
  assert!(!fixture.root().join("support/.core.partial").exists());

  let again = workspace.init(&mut shell, Runner::new(4)).unwrap();
  assert!(
    again
      .outcomes
      .iter()
      .all(|(_, o)| o.status == TaskStatus::Fresh)
  );
  let rendered = workspace.status(4).unwrap();
  assert!(
    rendered.iter().all(|s| !s.broken && s.flags.is_empty()),
    "{rendered:?}"
  );
}

#[test]
fn update_fast_forwards_and_restore_returns_to_the_lock() {
  let fixture = Fixture::new(&[("support/core", "main"), ("protocol/link", "main")]);
  let workspace = fixture.workspace();
  let mut shell = shell();
  workspace.init(&mut shell, Runner::new(2)).unwrap();
  workspace.relock(&mut shell, 2).unwrap();
  let locked = workspace.lock().unwrap();

  let newer = fixture.push_commit("support/core", "second");
  let report = workspace.update(&mut shell, Runner::new(2)).unwrap();
  assert_eq!(
    statuses(&report),
    [
      ("support/core", TaskStatus::Done),
      ("protocol/link", TaskStatus::Fresh)
    ]
  );
  let core = workspace.repo(&workspace.manifest.members[0]);
  assert_eq!(core.head().as_deref(), Some(newer.as_str()));
  let status = &workspace.status(2).unwrap()[0];
  assert_eq!(status.flags, ["unlocked"]);

  let report = workspace.restore(&mut shell, Runner::new(2)).unwrap();
  assert_eq!(
    report.outcome("support/core").unwrap().status,
    TaskStatus::Done
  );
  assert_eq!(core.head().as_deref(), locked.commit("support/core"));
  assert_eq!(core.current_branch(), None);

  std::fs::write(core.dir().join("README"), "local edit").unwrap();
  let report = workspace.update(&mut shell, Runner::new(2)).unwrap();
  let refused = report.outcome("support/core").unwrap();
  assert_eq!(refused.status, TaskStatus::Failed);
  assert!(refused.detail.contains("uncommitted changes"));
  assert!(
    workspace
      .relock(&mut shell, 2)
      .unwrap_err()
      .to_string()
      .contains("support/core: uncommitted changes")
  );
  assert!(
    report
      .finish(&mut shell, "updated", std::time::Instant::now())
      .is_err()
  );
}

#[test]
fn local_commits_are_reported_not_overwritten() {
  let fixture = Fixture::new(&[("support/core", "main")]);
  let workspace = fixture.workspace();
  let mut shell = shell();
  workspace.init(&mut shell, Runner::new(1)).unwrap();
  let clone = fixture.root().join("support/core");
  std::fs::write(clone.join("NOTES"), "mine").unwrap();
  git(&clone, &["add", "."]);
  git(&clone, &["commit", "-qm", "mine"]);
  let report = workspace.update(&mut shell, Runner::new(1)).unwrap();
  let outcome = report.outcome("support/core").unwrap();
  assert_eq!(outcome.status, TaskStatus::Warning);
  assert!(
    outcome.detail.contains("1 commit(s) ahead of origin/main"),
    "{}",
    outcome.detail
  );

  fixture.push_commit("support/core", "remote");
  let report = workspace.update(&mut shell, Runner::new(1)).unwrap();
  assert!(
    report
      .outcome("support/core")
      .unwrap()
      .detail
      .contains("diverged")
  );
}

#[test]
fn add_registers_clones_and_commits_the_workspace_files() {
  let fixture = Fixture::new(&[("support/core", "main"), ("protocol/link", "main")]);
  let mut workspace = fixture.workspace();
  let mut shell = shell();
  workspace.init(&mut shell, Runner::new(2)).unwrap();
  workspace.relock(&mut shell, 2).unwrap();
  fixture.remote("support/extra", "trunk");
  std::fs::write(
    fixture.root().join(Workspace::GITIGNORE),
    "/protocol/\n*.log\n",
  )
  .unwrap();

  let url = fixture.url("support/extra");
  let member = workspace.add(&mut shell, &url, None, None).unwrap();
  assert_eq!(
    (member.path.as_str(), member.branch.as_str()),
    ("support/extra", "trunk")
  );
  let paths: Vec<_> = workspace
    .manifest
    .members
    .iter()
    .map(|m| m.path.as_str())
    .collect();
  assert_eq!(paths, ["support/core", "support/extra", "protocol/link"]);
  assert!(workspace.lock().unwrap().commit("support/extra").is_some());
  assert_eq!(
    std::fs::read_to_string(fixture.root().join(Workspace::GITIGNORE)).unwrap(),
    "/protocol/\n/support/\n*.log\n"
  );
  assert!(
    workspace
      .add(&mut shell, &url, None, None)
      .unwrap_err()
      .to_string()
      .contains("already listed")
  );

  let root = fixture.root();
  git(&root, &["config", "user.name", "t"]);
  git(&root, &["config", "user.email", "t@example.com"]);
  let committer = Committer::new(&workspace);
  let files = [
    WorkspaceManifest::FILE,
    WorkspaceLock::FILE,
    Workspace::GITIGNORE,
  ];
  assert_eq!(committer.root_changes(&files).len(), 3);
  assert!(!workspace.ensure_ignored("support/other").unwrap());
  committer
    .commit_root(&mut shell, "FEAT: Add extra", &files, false)
    .unwrap();
  assert!(committer.root_changes(&files).is_empty());
  assert_eq!(
    Repo::new(&root)
      .query(&["log", "-1", "--format=%s"])
      .as_deref(),
    Some("FEAT: Add extra")
  );
}

#[test]
fn missing_workspaces_and_bad_origins_are_explained() {
  let empty = tempfile::tempdir().unwrap();
  let err = Workspace::discover(empty.path()).unwrap_err();
  assert!(err.to_string().contains("no `workspace.toml`"), "{err}");
  assert_eq!(err.exit_code(), Some(2));

  let fixture = Fixture::new(&[("support/core", "main")]);
  let workspace = fixture.workspace();
  let mut shell = shell();
  workspace.init(&mut shell, Runner::new(1)).unwrap();
  workspace.relock(&mut shell, 1).unwrap();
  let clone = fixture.root().join("support/core");
  git(
    &clone,
    &[
      "remote",
      "set-url",
      "origin",
      "https://example.invalid/other.git",
    ],
  );
  let report = workspace.restore(&mut shell, Runner::new(1)).unwrap();
  let outcome = report.outcome("support/core").unwrap();
  assert!(
    outcome
      .detail
      .contains("origin mismatch: https://example.invalid/other.git"),
    "{}",
    outcome.detail
  );
}
