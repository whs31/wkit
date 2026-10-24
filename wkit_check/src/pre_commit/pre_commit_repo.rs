#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreCommitRepo {
  pub repo: String,
  pub rev: Option<String>,
  pub line: usize,
  pub hooks: Vec<String>,
}

impl PreCommitRepo {
  pub fn parse_all(text: &str) -> Vec<Self> {
    let mut repos: Vec<Self> = Vec::new();
    for (index, raw) in text.lines().enumerate() {
      let line = raw.split(" #").next().unwrap_or_default().trim();
      let line = line.trim_start_matches("- ").trim();
      let Some((key, value)) = line.split_once(':') else {
        continue;
      };
      let value = value.trim().trim_matches(['"', '\'']).to_owned();
      match key.trim() {
        "repo" => repos.push(Self {
          repo: value,
          rev: None,
          line: index + 1,
          hooks: Vec::new(),
        }),
        "rev" => {
          if let Some(repo) = repos.last_mut() {
            repo.rev = Some(value);
            repo.line = index + 1;
          }
        }
        "id" => {
          if let Some(repo) = repos.last_mut() {
            repo.hooks.push(value);
          }
        }
        _ => {}
      }
    }
    repos
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reads_repos_revs_and_hooks() {
    let text = "repos:\n  - repo: local\n    hooks:\n      - id: just-fmt\n        entry: just fmt\n  - repo: https://github.com/pre-commit/mirrors-clang-format\n    rev: v23.1.3  # keep in sync\n    hooks:\n      - id: clang-format\n        args: [--style=file]\n";
    let repos = PreCommitRepo::parse_all(text);
    assert_eq!(repos.len(), 2);
    assert_eq!(repos[0].hooks, ["just-fmt"]);
    assert_eq!(repos[0].rev, None);
    assert_eq!(repos[1].rev.as_deref(), Some("v23.1.3"));
    assert_eq!(repos[1].hooks, ["clang-format"]);
    assert_eq!(repos[1].line, 7);
  }
}
