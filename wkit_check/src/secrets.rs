mod secret_kind;

use wkit_core::{Shell, fs::SourceSet};

use self::secret_kind::SecretKind;
use crate::{Check, CheckOptions, Outcome, Result};

#[derive(Debug, Clone)]
pub struct Secrets {
  root: std::path::PathBuf,
}

impl Secrets {
  pub const ALLOW_MARKER: &str = "wkit: allow-secret";
  pub const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;

  pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
    Self { root: root.into() }
  }

  pub fn scan(text: &str) -> Vec<(usize, &'static str)> {
    static SET: std::sync::LazyLock<regex::RegexSet> = std::sync::LazyLock::new(|| {
      regex::RegexSet::new(SecretKind::ALL.iter().map(|k| k.pattern)).expect("valid")
    });
    text
      .lines()
      .enumerate()
      .filter(|(_, line)| !line.contains(Self::ALLOW_MARKER))
      .flat_map(|(index, line)| {
        SET
          .matches(line)
          .into_iter()
          .map(move |kind| (index + 1, SecretKind::ALL[kind].name))
      })
      .collect()
  }

  fn readable(path: &std::path::Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > Self::MAX_FILE_SIZE {
      return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if bytes[..bytes.len().min(8192)].contains(&0) {
      return None;
    }
    String::from_utf8(bytes).ok()
  }
}

impl Check for Secrets {
  fn name(&self) -> String {
    "secrets".to_owned()
  }

  fn run(&self, shell: &mut Shell, _: &CheckOptions) -> Result<Outcome> {
    let files = SourceSet::new(&self.root).hidden(true).collect()?;
    shell.status(
      "Checking",
      format_args!("{} for secrets", Outcome::plural(files.len(), "file")),
    )?;
    let mut details = Vec::new();
    for file in &files {
      let Some(text) = Self::readable(file) else {
        continue;
      };
      let shown = file.strip_prefix(&self.root).unwrap_or(file).display();
      for (line, kind) in Self::scan(&text) {
        details.push(format!("  {shown}:{line}: {kind}"));
      }
    }
    if details.is_empty() {
      return Ok(Outcome::passed(format!(
        "{} clean",
        Outcome::plural(files.len(), "file")
      )));
    }
    details.push(format!(
      "  mark intentional test data with `{}` on the same line",
      Self::ALLOW_MARKER
    ));
    Ok(Outcome::failed(
      format!("{} found", Outcome::plural(details.len() - 1, "secret")),
      details,
    ))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn finds_tokens_and_keys() {
    let github = format!("token = \"ghp_{}\"", "a".repeat(36));
    let gitlab = format!("glpat-{}", "x".repeat(20));
    let key = format!("-----BEGIN {}PRIVATE KEY-----", "RSA ");
    let text = format!("ok\n{github}\n{gitlab} {}\n{key}\n", Secrets::ALLOW_MARKER);
    assert_eq!(
      Secrets::scan(&text),
      [(2, "GitHub token"), (4, "private key")]
    );
    assert!(Secrets::scan("ghp_short glpat- AKIA").is_empty());
  }

  #[test]
  fn reports_files_relative_to_the_root() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
      dir.path().join(".env"),
      format!("AWS={}\n", "AKIA".to_owned() + &"Q".repeat(16)),
    )
    .unwrap();
    std::fs::write(dir.path().join("blob.bin"), b"\0AKIAQQQQQQQQQQQQQQQQ").unwrap();
    let outcome = Secrets::new(dir.path())
      .run(
        &mut Shell::from_write(Box::new(std::io::sink())),
        &CheckOptions::default(),
      )
      .unwrap();
    assert_eq!(outcome.status, crate::Status::Failed);
    assert_eq!(outcome.details[0], "  .env:1: AWS access key");
  }
}
