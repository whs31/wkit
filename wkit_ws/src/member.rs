#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
  pub path: String,
  pub url: String,
  pub branch: String,
}

impl Member {
  pub fn new(path: impl Into<String>, url: impl Into<String>, branch: impl Into<String>) -> Self {
    Self {
      path: path.into(),
      url: url.into(),
      branch: branch.into(),
    }
  }

  pub fn dir(&self, root: &std::path::Path) -> std::path::PathBuf {
    root.join(&self.path)
  }

  pub fn name(&self) -> &str {
    self.path.rsplit('/').next().unwrap_or(&self.path)
  }

  pub fn is_valid_path(path: &str) -> bool {
    !path.is_empty()
      && !path.starts_with('/')
      && !path.contains('\\')
      && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
  }

  pub fn is_valid_branch(branch: &str) -> bool {
    !branch.is_empty() && !branch.starts_with('-') && !branch.chars().any(char::is_whitespace)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

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
}
