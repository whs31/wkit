mod pre_commit_repo;

use wkit_core::Shell;
use wkit_manifest::Tools;

use self::pre_commit_repo::PreCommitRepo;
use crate::{Check, CheckOptions, Outcome, Result};

#[derive(Debug, Clone)]
pub struct PreCommitPins {
  root: std::path::PathBuf,
  tools: Tools,
}

impl PreCommitPins {
  pub const CONFIGS: &[&str] = &[".pre-commit-config.yaml", ".pre-commit-config.yml"];

  pub fn new(root: impl Into<std::path::PathBuf>, tools: Tools) -> Self {
    Self {
      root: root.into(),
      tools,
    }
  }

  pub fn config(&self) -> Option<std::path::PathBuf> {
    Self::CONFIGS
      .iter()
      .map(|name| self.root.join(name))
      .find(|path| path.is_file())
  }

  pub fn mismatches(&self, config: &str, text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for repo in PreCommitRepo::parse_all(text) {
      let Some(rev) = &repo.rev else {
        continue;
      };
      for hook in &repo.hooks {
        let pinned =
          self.tools.get(hook).is_some() || Tools::DEFAULTS.iter().any(|(t, _)| t == hook);
        let tool = self.tools.resolve(hook, &self.root);
        let Some(version) = tool.version().filter(|_| pinned) else {
          continue;
        };
        if rev.trim_start_matches('v') != version {
          found.push(format!(
            "  {config}:{}: {hook} is pinned to {rev}, but `[tools]` uses {version}",
            repo.line
          ));
        }
      }
    }
    found
  }
}

impl Check for PreCommitPins {
  fn name(&self) -> String {
    "pre-commit".to_owned()
  }

  fn run(&self, shell: &mut Shell, _: &CheckOptions) -> Result<Outcome> {
    let Some(path) = self.config() else {
      return Ok(Outcome::skipped("no .pre-commit-config.yaml"));
    };
    let name = path
      .file_name()
      .map(|n| n.to_string_lossy().into_owned())
      .unwrap_or_default();
    shell.status("Checking", format_args!("tool pins in {name}"))?;
    let text = wkit_core::fs::read_to_string(&path)?;
    let mismatches = self.mismatches(&name, &text);
    Ok(if mismatches.is_empty() {
      Outcome::passed("pins match `[tools]`")
    } else {
      Outcome::failed(Outcome::plural(mismatches.len(), "stale pin"), mismatches)
    })
  }
}

#[cfg(test)]
mod tests {
  use wkit_manifest::Manifest;

  use super::*;

  #[test]
  fn compares_revs_with_pinned_tools() {
    let tools = Manifest::parse("[project]\nlanguages = [\"cpp\"]\n[tools]\nblack = \"25.9.0\"\n")
      .unwrap()
      .manifest
      .tools;
    let pins = PreCommitPins::new("/repo", tools);
    let text = "repos:\n  - repo: https://github.com/pre-commit/mirrors-clang-format\n    rev: v19.1.6\n    hooks:\n      - id: clang-format\n  - repo: https://github.com/psf/black\n    rev: 25.9.0\n    hooks:\n      - id: black\n  - repo: https://github.com/pre-commit/pre-commit-hooks\n    rev: v5.0.0\n    hooks:\n      - id: trailing-whitespace\n";
    assert_eq!(
      pins.mismatches(".pre-commit-config.yaml", text),
      ["  .pre-commit-config.yaml:3: clang-format is pinned to v19.1.6, but `[tools]` uses 23.1.3"]
    );
  }
}
