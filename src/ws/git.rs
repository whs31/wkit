use std::{
  io::{Read as _, Write as _},
  path::{Path, PathBuf},
  process::{Command, Stdio},
};

use super::Phase;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRun {
  pub success: bool,
  pub output: String,
}

impl GitRun {
  fn new(result: std::io::Result<std::process::Output>) -> Self {
    match result {
      Ok(output) => Self {
        success: output.status.success(),
        output: format!(
          "{}{}",
          String::from_utf8_lossy(&output.stdout),
          String::from_utf8_lossy(&output.stderr)
        ),
      },
      Err(e) => Self {
        success: false,
        output: format!("cannot run git: {e}"),
      },
    }
  }

  pub fn fatal(&self) -> Option<&str> {
    self
      .output
      .lines()
      .rev()
      .find_map(|line| line.strip_prefix("fatal: "))
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
  dir: PathBuf,
}

impl Repo {
  pub fn new(dir: impl Into<PathBuf>) -> Self {
    Self { dir: dir.into() }
  }

  pub fn dir(&self) -> &Path {
    &self.dir
  }

  fn command(&self, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
      .current_dir(&self.dir)
      .env("GIT_TERMINAL_PROMPT", "0")
      .stdin(Stdio::null())
      .args(args);
    command
  }

  pub fn git(&self, args: &[&str]) -> GitRun {
    GitRun::new(self.command(args).output())
  }

  pub fn git_with_input(&self, args: &[&str], input: &str) -> GitRun {
    let child = self
      .command(args)
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .spawn();
    GitRun::new(child.and_then(|mut child| {
      if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
      }
      child.wait_with_output()
    }))
  }

  pub fn query_raw(&self, args: &[&str]) -> Option<String> {
    let output = self.command(args).output().ok()?;
    output
      .status
      .success()
      .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
  }

  pub fn query(&self, args: &[&str]) -> Option<String> {
    self.query_raw(args).map(|out| out.trim().to_owned())
  }

  pub fn with_progress(&self, args: &[&str], phase: &Phase, label: &str) -> GitRun {
    phase.set(label, None);
    let mut kept = Vec::new();
    let mut nested = String::new();
    let mut handle = |line: &str| {
      if let Some((_, name, percent)) =
        lazy_regex::regex_captures!(r"^(?:remote:\s*)?([A-Za-z][A-Za-z ]*?):\s+(\d{1,3})%", line)
      {
        let percent: f64 = percent.parse().unwrap_or_default();
        phase.set(
          &format!("{nested}{}", name.to_lowercase()),
          Some(percent / 100.0),
        );
        return;
      }
      if let Some((_, path)) = lazy_regex::regex_captures!(r"^Cloning into '(.+)'", line) {
        nested = format!("{}: ", path.rsplit('/').next().unwrap_or(path));
      }
      if !line.trim().is_empty() {
        kept.push(line.to_owned());
      }
    };
    let status = (|| {
      let (mut reader, writer) = std::io::pipe()?;
      let mut command = self.command(args);
      let mut child = command.stdout(writer.try_clone()?).stderr(writer).spawn()?;
      drop(command);
      let (mut pending, mut buffer) = (Vec::new(), [0; 8192]);
      loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
          break;
        }
        pending.extend_from_slice(&buffer[..read]);
        while let Some(end) = pending.iter().position(|&b| b == b'\n' || b == b'\r') {
          let line: Vec<u8> = pending.drain(..=end).collect();
          handle(&String::from_utf8_lossy(&line[..end]));
        }
      }
      handle(&String::from_utf8_lossy(&pending));
      child.wait()
    })();
    match status {
      Ok(status) => GitRun {
        success: status.success(),
        output: kept.join("\n"),
      },
      Err(e) => GitRun {
        success: false,
        output: format!("cannot run git: {e}"),
      },
    }
  }

  pub fn exists(&self) -> bool {
    self.dir.exists()
  }

  pub fn is_repository(&self) -> bool {
    self.dir.is_dir() && self.git(&["rev-parse", "--is-inside-work-tree"]).success
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
      .git(&["merge-base", "--is-ancestor", ancestor, descendant])
      .success
  }

  pub fn is_dirty(&self) -> bool {
    self
      .query(&["status", "--porcelain", "--untracked-files=all"])
      .is_none_or(|status| !status.is_empty())
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
    let (ahead, behind) = counts.split_once(char::is_whitespace)?;
    Some((ahead.parse().ok()?, behind.trim().parse().ok()?))
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

  fn ls_remote(args: &[&str]) -> String {
    Self::new(std::env::temp_dir())
      .query_raw(&[&["ls-remote"], args].concat())
      .unwrap_or_default()
  }

  pub fn remote_default_branch(url: &str) -> Option<String> {
    Self::ls_remote(&["--symref", url, "HEAD"])
      .lines()
      .find_map(|line| {
        line
          .strip_prefix("ref: refs/heads/")
          .and_then(|rest| rest.strip_suffix("\tHEAD"))
          .map(ToOwned::to_owned)
      })
  }

  pub fn remote_branch_head(url: &str, branch: &str) -> Option<String> {
    let text = Self::ls_remote(&[url, &format!("refs/heads/{branch}")]);
    let (sha, _) = text.lines().next()?.split_once('\t')?;
    super::is_sha(sha).then(|| sha.to_owned())
  }
}
