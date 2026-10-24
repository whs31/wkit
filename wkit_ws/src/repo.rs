use crate::{GitRun, Phase};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
  dir: std::path::PathBuf,
}

impl Repo {
  pub fn new(dir: impl Into<std::path::PathBuf>) -> Self {
    Self { dir: dir.into() }
  }

  pub fn dir(&self) -> &std::path::Path {
    &self.dir
  }

  pub fn command(dir: &std::path::Path) -> std::process::Command {
    let mut command = std::process::Command::new("git");
    command
      .current_dir(dir)
      .env("GIT_TERMINAL_PROMPT", "0")
      .stdin(std::process::Stdio::null());
    command
  }

  pub fn git<I, S>(&self, args: I) -> GitRun
  where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
  {
    match Self::command(&self.dir).args(args).output() {
      Ok(output) => GitRun::from_output(&output),
      Err(e) => GitRun::failure(format!("cannot run git: {e}")),
    }
  }

  pub fn git_with_input(&self, args: &[&str], input: &str) -> GitRun {
    let child = Self::command(&self.dir)
      .args(args)
      .stdin(std::process::Stdio::piped())
      .stdout(std::process::Stdio::piped())
      .stderr(std::process::Stdio::piped())
      .spawn();
    let mut child = match child {
      Ok(child) => child,
      Err(e) => return GitRun::failure(format!("cannot run git: {e}")),
    };
    if let Some(mut stdin) = child.stdin.take() {
      let _ = std::io::Write::write_all(&mut stdin, input.as_bytes());
    }
    match child.wait_with_output() {
      Ok(output) => GitRun::from_output(&output),
      Err(e) => GitRun::failure(format!("cannot run git: {e}")),
    }
  }

  pub fn query(&self, args: &[&str]) -> Option<String> {
    self.query_raw(args).map(|out| out.trim().to_owned())
  }

  pub fn query_raw(&self, args: &[&str]) -> Option<String> {
    let output = Self::command(&self.dir).args(args).output().ok()?;
    output
      .status
      .success()
      .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
  }

  pub fn with_progress(&self, args: &[&str], phase: &Phase, label: &str) -> GitRun {
    static PROGRESS: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(r"^(?:remote:\s*)?([A-Za-z][A-Za-z ]*?):\s+(\d{1,3})%").expect("valid")
    });
    static CLONING: std::sync::LazyLock<regex::Regex> =
      std::sync::LazyLock::new(|| regex::Regex::new(r"^Cloning into '(.+)'").expect("valid"));
    phase.set(label, None);
    let mut command = Self::command(&self.dir);
    let child = command
      .args(args)
      .stdout(std::process::Stdio::piped())
      .stderr(std::process::Stdio::piped())
      .spawn();
    let mut child = match child {
      Ok(child) => child,
      Err(e) => return GitRun::failure(format!("cannot run git: {e}")),
    };
    let stdout = child.stdout.take().expect("piped");
    let collector = std::thread::spawn(move || {
      let mut text = String::new();
      let _ = std::io::Read::read_to_string(&mut { stdout }, &mut text);
      text
    });
    let mut stderr = child.stderr.take().expect("piped");
    let mut kept = Vec::new();
    let mut pending = Vec::new();
    let mut nested = String::new();
    let mut buffer = [0; 8192];
    let mut handle = |line: &str, kept: &mut Vec<String>| {
      if let Some(c) = PROGRESS.captures(line) {
        let percent: f64 = c[2].parse().unwrap_or_default();
        phase.set(
          &format!("{nested}{}", c[1].to_lowercase()),
          Some(percent / 100.0),
        );
        return;
      }
      if let Some(c) = CLONING.captures(line) {
        nested = format!("{}: ", c[1].rsplit('/').next().unwrap_or(&c[1]));
      }
      if !line.trim().is_empty() {
        kept.push(line.to_owned());
      }
    };
    while let Ok(read) = std::io::Read::read(&mut stderr, &mut buffer) {
      if read == 0 {
        break;
      }
      pending.extend_from_slice(&buffer[..read]);
      while let Some(end) = pending.iter().position(|&b| b == b'\n' || b == b'\r') {
        let line: Vec<u8> = pending.drain(..=end).collect();
        handle(String::from_utf8_lossy(&line[..end]).as_ref(), &mut kept);
      }
    }
    handle(String::from_utf8_lossy(&pending).as_ref(), &mut kept);
    let status = child.wait();
    let stdout = collector.join().unwrap_or_default();
    kept.extend(
      stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(ToOwned::to_owned),
    );
    GitRun {
      success: status.is_ok_and(|s| s.success()),
      output: kept.join("\n"),
    }
  }

  pub fn exists(&self) -> bool {
    self.dir.exists()
  }

  pub fn is_repository(&self) -> bool {
    self.dir.is_dir() && self.git(["rev-parse", "--is-inside-work-tree"]).success
  }

  pub fn is_standalone(&self) -> bool {
    self.dir.join(".git").is_dir()
  }

  pub fn revision(&self, reference: &str) -> Option<String> {
    self.query(&[
      "rev-parse",
      "--verify",
      "--quiet",
      &format!("{reference}^{{commit}}"),
    ])
  }

  pub fn head(&self) -> Option<String> {
    self.revision("HEAD")
  }

  pub fn current_branch(&self) -> Option<String> {
    self.query(&["symbolic-ref", "--quiet", "--short", "HEAD"])
  }

  pub fn upstream(&self) -> Option<String> {
    self.query(&[
      "rev-parse",
      "--abbrev-ref",
      "--symbolic-full-name",
      "@{upstream}",
    ])
  }

  pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> bool {
    self
      .git(["merge-base", "--is-ancestor", ancestor, descendant])
      .success
  }

  pub fn is_dirty(&self) -> bool {
    match self.query(&["status", "--porcelain", "--untracked-files=all"]) {
      Some(status) => !status.is_empty(),
      None => true,
    }
  }

  pub fn is_published(&self, commit: &str) -> bool {
    self
      .query(&[
        "for-each-ref",
        "--count=1",
        &format!("--contains={commit}"),
        "refs/remotes/origin",
      ])
      .is_some_and(|refs| !refs.is_empty())
  }

  pub fn origin_url(&self) -> Option<String> {
    self.query(&["remote", "get-url", "origin"])
  }

  pub fn ahead_behind(&self, upstream: &str) -> Option<(u64, u64)> {
    let counts = self.query(&[
      "rev-list",
      "--left-right",
      "--count",
      &format!("HEAD...{upstream}"),
    ])?;
    let mut parts = counts.split_whitespace().map(str::parse::<u64>);
    Some((parts.next()?.ok()?, parts.next()?.ok()?))
  }

  pub fn changed_files(&self) -> Vec<String> {
    self
      .query(&[
        "diff",
        "-z",
        "--name-only",
        "--ignore-submodules=all",
        "HEAD",
      ])
      .map(|out| {
        out
          .split('\0')
          .filter(|n| !n.is_empty())
          .map(ToOwned::to_owned)
          .collect()
      })
      .unwrap_or_default()
  }

  pub fn remote_default_branch(url: &str) -> Option<String> {
    let output = Self::command(&std::env::temp_dir())
      .args(["ls-remote", "--symref", url, "HEAD"])
      .output()
      .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().find_map(|line| {
      line
        .strip_prefix("ref: refs/heads/")
        .and_then(|rest| rest.strip_suffix("\tHEAD"))
        .map(ToOwned::to_owned)
    })
  }

  pub fn remote_branch_head(url: &str, branch: &str) -> Option<String> {
    let output = Self::command(&std::env::temp_dir())
      .args(["ls-remote", url, &format!("refs/heads/{branch}")])
      .output()
      .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let (sha, _) = text.lines().next()?.split_once('\t')?;
    crate::WorkspaceLock::is_sha(sha).then(|| sha.to_owned())
  }

  pub fn short(sha: Option<&str>) -> &str {
    sha.map_or("--------", |s| &s[..s.len().min(8)])
  }
}
