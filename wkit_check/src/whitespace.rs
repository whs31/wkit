use wkit_core::{ProcessBuilder, Shell};

use crate::{Check, CheckOptions, Outcome, Result};

#[derive(Debug, Clone)]
pub struct Whitespace {
  root: std::path::PathBuf,
}

impl Whitespace {
  pub const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

  pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
    Self { root: root.into() }
  }

  pub fn problems(output: &str) -> Vec<String> {
    static RE: std::sync::LazyLock<regex::Regex> =
      std::sync::LazyLock::new(|| regex::Regex::new(r"^(.+?):(\d+): (.+?)\.?$").expect("valid"));
    output
      .lines()
      .filter(|line| !line.starts_with(['+', '-', ' ']))
      .filter_map(|line| RE.captures(line))
      .map(|c| format!("  {}:{}: {}", &c[1], &c[2], &c[3]))
      .collect()
  }

  fn git(&self) -> ProcessBuilder {
    ProcessBuilder::new("git").cwd(&self.root)
  }
}

impl Check for Whitespace {
  fn name(&self) -> String {
    "whitespace".to_owned()
  }

  fn run(&self, shell: &mut Shell, _: &CheckOptions) -> Result<Outcome> {
    let inside = self
      .git()
      .args(["rev-parse", "--is-inside-work-tree"])
      .output_unchecked();
    if !inside.is_ok_and(|o| o.status.success()) {
      return Ok(Outcome::skipped("not a git work tree"));
    }
    shell.status("Checking", "whitespace in uncommitted changes")?;
    let has_head = self
      .git()
      .args(["rev-parse", "--quiet", "--verify", "HEAD"])
      .output_unchecked()?
      .status
      .success();
    let base = if has_head { "HEAD" } else { Self::EMPTY_TREE };
    let diff = self.git().args(["diff", "--check", base, "--", "."]);
    let output = diff.output_unchecked()?;
    if output.status.success() {
      return Ok(Outcome::passed("clean"));
    }
    let problems = Self::problems(&String::from_utf8_lossy(&output.stdout));
    if problems.is_empty() {
      return Err(
        wkit_core::Error::ProcessFailed {
          command: diff.to_string(),
          status: output.status,
          stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
        .into(),
      );
    }
    Ok(Outcome::failed(
      Outcome::plural(problems.len(), "problem"),
      problems,
    ))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_git_diff_check() {
    let output = "src/a.cpp:3: trailing whitespace.\n+int x; \nREADME.md:10: space before tab in indent.\n+ \tfoo\n";
    assert_eq!(
      Whitespace::problems(output),
      [
        "  src/a.cpp:3: trailing whitespace",
        "  README.md:10: space before tab in indent"
      ]
    );
  }
}
