use std::{fmt::Write as _, path::Path};

use super::*;

const MANIFEST: &str = r#"# Ordered project catalog.
version = 1

[[project]]
path = "support/rcommon"
url = "http://git.example/dev/support/rcommon.git"
branch = "main"

[[project]]
path = "protocol/xlink"
url = "http://git.example/dev/protocol/xlink.git"
branch = "main"
"#;

const A: &str = "f047e85e4896be52e55d70750a0a530087301b42";
const B: &str = "fd828533ee7a4f22ccd8eceac3625aaf7dcd253c";

fn problem(text: &str) -> String {
  Manifest::parse(text).unwrap_err().to_string()
}

#[test]
fn paths_must_be_normalized_and_relative() {
  for path in ["support/rcommon", "x"] {
    assert!(Member::is_valid_path(path), "{path}");
  }
  for path in ["", "/abs", "a//b", "a/./b", "../up", "a/", "a\\b"] {
    assert!(!Member::is_valid_path(path), "{path}");
  }
}

#[test]
fn branches_reject_options_and_spaces() {
  assert!(Member::is_valid_branch("feature/x"));
  assert!(
    !Member::is_valid_branch("-f")
      && !Member::is_valid_branch("a b")
      && !Member::is_valid_branch("")
  );
  assert_eq!(Member::new("protocol/xlink", "u", "main").name(), "xlink");
}

#[test]
fn reads_the_ordered_catalog() {
  let manifest = Manifest::parse(MANIFEST).unwrap();
  let paths: Vec<_> = manifest.members.iter().map(|m| m.path.as_str()).collect();
  assert_eq!(paths, ["support/rcommon", "protocol/xlink"]);
  assert_eq!(manifest.member("protocol/xlink").unwrap().branch, "main");
}

#[test]
fn rejects_what_the_python_tooling_rejects() {
  assert!(
    problem("version = 2\n[[project]]\npath='a'\nurl='u'\nbranch='m'\n")
      .contains("`version` must be 1")
  );
  assert!(problem("version = 1\n").contains("one or more"));
  assert!(
    problem("version = 1\n[[project]]\npath='../a'\nurl='u'\nbranch='m'\n").contains("normalized")
  );
  assert!(problem("version = 1\n[[project]]\npath='a'\nurl=''\nbranch='m'\n").contains("`url`"));
  assert!(
    problem("version = 1\n[[project]]\npath='a'\nurl='u'\nbranch='-x'\n")
      .contains("invalid branch")
  );
  let twice = "version = 1\n[[project]]\npath='a'\nurl='u'\nbranch='m'\n[[project]]\npath='a'\nurl='v'\nbranch='m'\n";
  assert!(problem(twice).contains("duplicate project path: a"));
  let nested = "version = 1\n[[project]]\npath='a'\nurl='u'\nbranch='m'\n[[project]]\npath='a/b'\nurl='v'\nbranch='m'\n";
  assert!(problem(nested).contains("overlapping project paths: a and a/b"));
  let siblings = "version = 1\n[[project]]\npath='a'\nurl='u'\nbranch='m'\n[[project]]\npath='ab'\nurl='v'\nbranch='m'\n";
  assert!(Manifest::parse(siblings).is_ok());
}

#[test]
fn new_paths_follow_the_shared_url_layout() {
  let manifest = Manifest::parse(MANIFEST).unwrap();
  assert_eq!(
    manifest.derive_path("http://git.example/dev/protocol/xschema.git"),
    "protocol/xschema"
  );
  assert_eq!(
    manifest.derive_path("https://github.com/x/tool.git/"),
    "tool"
  );
}

#[test]
fn new_members_land_next_to_their_group() {
  let manifest = Manifest::parse(MANIFEST).unwrap();
  let member = Member::new(
    "support/gatekeep",
    "http://git.example/dev/support/gatekeep.git",
    "main",
  );
  let text = manifest.with_member(MANIFEST, &member);
  let added = Manifest::parse(&text).unwrap();
  let paths: Vec<_> = added.members.iter().map(|m| m.path.as_str()).collect();
  assert_eq!(
    paths,
    ["support/rcommon", "support/gatekeep", "protocol/xlink"]
  );
  assert!(
    text.contains("branch = \"main\"\n\n[[project]]\npath = \"protocol/xlink\""),
    "{text}"
  );

  let other = Member::new("tools/x", "u", "dev");
  let text = manifest.with_member(MANIFEST.trim_end(), &other);
  assert!(
    text.ends_with(
      "branch = \"main\"\n\n[[project]]\npath = \"tools/x\"\nurl = \"u\"\nbranch = \"dev\"\n"
    ),
    "{text}"
  );
}

fn two_members() -> Manifest {
  Manifest::parse("version = 1\n[[project]]\npath='b/one'\nurl='u'\nbranch='m'\n[[project]]\npath='a/two'\nurl='v'\nbranch='m'\n")
    .unwrap()
}

#[test]
fn locks_render_in_manifest_order_and_read_back() {
  let mut lock = Lock::default();
  lock.revisions.insert("b/one".into(), A.into());
  lock.revisions.insert("a/two".into(), B.into());
  let text = lock.render(&two_members()).unwrap();
  assert_eq!(
    text,
    format!(
      "{LOCK_HEADER}\nversion = 1\n\n[[revision]]\npath = \"b/one\"\ncommit = \"{A}\"\n\n[[revision]]\npath = \"a/two\"\ncommit = \"{B}\"\n"
    )
  );
  let read = Lock::parse(&text).unwrap();
  assert_eq!(read, lock);
  read.check(&two_members()).unwrap();
}

#[test]
fn locks_must_cover_the_manifest_exactly() {
  let one = Lock::parse(&format!(
    "version = 1\n[[revision]]\npath = \"b/one\"\ncommit = \"{A}\"\n"
  ))
  .unwrap();
  assert!(
    one
      .check(&two_members())
      .unwrap_err()
      .to_string()
      .contains("missing projects: a/two")
  );
  let extra = format!(
    "version = 1\n[[revision]]\npath = \"b/one\"\ncommit = \"{A}\"\n[[revision]]\npath = \"a/two\"\ncommit = \"{A}\"\n[[revision]]\npath = \"c\"\ncommit = \"{A}\"\n"
  );
  let err = Lock::parse(&extra)
    .unwrap()
    .check(&two_members())
    .unwrap_err();
  assert!(err.to_string().contains("unknown projects: c"), "{err}");
  let bad = Lock::parse("version = 1\n[[revision]]\npath = \"x\"\ncommit = \"ABC\"\n").unwrap_err();
  assert!(bad.to_string().contains("40-character SHA"));
  assert_eq!(Failure::exit_code(&bad), 2);
}

fn git(dir: &Path, args: &[&str]) {
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

fn init(dir: &Path, flags: &[&str], branch: &str) {
  git(dir, &[&["init", "-q"], flags].concat());
  git(
    dir,
    &["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")],
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
    std::fs::write(root.join(MANIFEST_FILE), manifest).unwrap();
    init(&root, &[], "main");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "workspace"]);
    fixture
  }

  fn root(&self) -> PathBuf {
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
      .replace('\\', "/")
  }

  fn seed(&self, path: &str) -> PathBuf {
    self.dir.path().join("seed").join(path)
  }

  fn remote(&self, path: &str, branch: &str) {
    let url = self.url(path);
    std::fs::create_dir_all(&url).unwrap();
    init(Path::new(&url), &["--bare"], branch);
    let seed = self.seed(path);
    std::fs::create_dir_all(&seed).unwrap();
    init(&seed, &[], branch);
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

fn statuses(report: &Report) -> Vec<(&str, TaskStatus)> {
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
  let mut shell = Shell::sink();
  assert!(!workspace.is_fully_locked());
  let report = workspace.init(&mut shell, 4).unwrap();
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

  let again = workspace.init(&mut shell, 4).unwrap();
  assert!(
    again
      .outcomes
      .iter()
      .all(|(_, o)| o.status == TaskStatus::Fresh)
  );
  let rendered = workspace.status(4);
  assert!(
    rendered.iter().all(|s| !s.broken && s.flags.is_empty()),
    "{rendered:?}"
  );
}

#[test]
fn update_fast_forwards_and_restore_returns_to_the_lock() {
  let fixture = Fixture::new(&[("support/core", "main"), ("protocol/link", "main")]);
  let workspace = fixture.workspace();
  let mut shell = Shell::sink();
  workspace.init(&mut shell, 2).unwrap();
  workspace.relock(&mut shell, 2).unwrap();
  let locked = workspace.lock().unwrap();

  let newer = fixture.push_commit("support/core", "second");
  let report = workspace.update(&mut shell, 2).unwrap();
  assert_eq!(
    statuses(&report),
    [
      ("support/core", TaskStatus::Done),
      ("protocol/link", TaskStatus::Fresh)
    ]
  );
  let core = workspace.repo(&workspace.manifest.members[0]);
  assert_eq!(core.head().as_deref(), Some(newer.as_str()));
  assert_eq!(workspace.status(2)[0].flags, ["unlocked"]);

  let report = workspace.restore(&mut shell, 2).unwrap();
  assert_eq!(
    report.outcome("support/core").unwrap().status,
    TaskStatus::Done
  );
  assert_eq!(core.head().as_deref(), locked.commit("support/core"));
  assert_eq!(core.current_branch(), None);

  std::fs::write(core.dir().join("README"), "local edit").unwrap();
  let report = workspace.update(&mut shell, 2).unwrap();
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
  let err = report
    .finish(&mut shell, "updated", std::time::Instant::now())
    .unwrap_err();
  assert_eq!(Failure::exit_code(&err), 1);
}

#[test]
fn local_commits_are_reported_not_overwritten() {
  let fixture = Fixture::new(&[("support/core", "main")]);
  let workspace = fixture.workspace();
  let mut shell = Shell::sink();
  workspace.init(&mut shell, 1).unwrap();
  let clone = fixture.root().join("support/core");
  std::fs::write(clone.join("NOTES"), "mine").unwrap();
  git(&clone, &["add", "."]);
  git(&clone, &["commit", "-qm", "mine"]);
  let report = workspace.update(&mut shell, 1).unwrap();
  let outcome = report.outcome("support/core").unwrap();
  assert_eq!(outcome.status, TaskStatus::Warning);
  assert!(
    outcome.detail.contains("1 commit(s) ahead of origin/main"),
    "{}",
    outcome.detail
  );

  fixture.push_commit("support/core", "remote");
  let report = workspace.update(&mut shell, 1).unwrap();
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
  let mut shell = Shell::sink();
  workspace.init(&mut shell, 2).unwrap();
  workspace.relock(&mut shell, 2).unwrap();
  fixture.remote("support/extra", "trunk");
  std::fs::write(fixture.root().join(GITIGNORE), "/protocol/\n*.log\n").unwrap();

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
    std::fs::read_to_string(fixture.root().join(GITIGNORE)).unwrap(),
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
  let files = [MANIFEST_FILE, LOCK_FILE, GITIGNORE];
  assert_eq!(committer.root_changes(&files).len(), 3);
  assert!(!workspace.ensure_ignored("support/other").unwrap());
  committer
    .commit_root(&mut shell, "FEAT: Add extra", &files, false)
    .unwrap();
  assert_eq!(committer.root_changes(&files), Vec::<String>::new());
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
  assert_eq!(Failure::exit_code(&err), 2);

  let fixture = Fixture::new(&[("support/core", "main")]);
  let workspace = fixture.workspace();
  let mut shell = Shell::sink();
  workspace.init(&mut shell, 1).unwrap();
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
  let report = workspace.restore(&mut shell, 1).unwrap();
  let outcome = report.outcome("support/core").unwrap();
  assert!(
    outcome
      .detail
      .contains("origin mismatch: https://example.invalid/other.git"),
    "{}",
    outcome.detail
  );
}
