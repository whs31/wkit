#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellQuoted<'a>(pub &'a str);

impl ShellQuoted<'_> {
  fn is_plain(self) -> bool {
    !self.0.is_empty()
      && self
        .0
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_./=:+,@%".contains(c))
  }
}

impl std::fmt::Display for ShellQuoted<'_> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    if self.is_plain() {
      f.write_str(self.0)
    } else {
      write!(f, "'{}'", self.0.replace('\'', r"'\''"))
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn quotes_only_when_needed() {
    assert_eq!(ShellQuoted("build/Debug").to_string(), "build/Debug");
    assert_eq!(ShellQuoted("&:test=True").to_string(), "'&:test=True'");
    assert_eq!(ShellQuoted("").to_string(), "''");
    assert_eq!(ShellQuoted("it's").to_string(), r"'it'\''s'");
  }
}
