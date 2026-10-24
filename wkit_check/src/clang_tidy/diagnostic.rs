#[derive(Debug, Clone)]
pub struct Diagnostic {
  pub path: std::path::PathBuf,
  pub line: u32,
  pub column: u32,
  pub severity: String,
  pub message: String,
  pub check: String,
  pub details: Vec<String>,
}

impl Diagnostic {
  pub fn parse_all(output: &str, base: &std::path::Path) -> Vec<Self> {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(
        r"^(?<path>.+?):(?<line>\d+):(?<column>\d+): (?<severity>fatal error|error|warning|note): (?<message>.*?)(?: \[(?<check>[^\]]+)\])?$",
      )
      .expect("valid")
    });
    let mut found: Vec<Self> = Vec::new();
    let mut open = false;
    for line in output.lines() {
      let Some(c) = RE.captures(line).filter(|c| &c["severity"] != "note") else {
        if open && !line.trim().is_empty() && !line.ends_with(" generated.") {
          found
            .last_mut()
            .expect("open")
            .details
            .push(line.to_owned());
        }
        continue;
      };
      let check = c.name("check").map_or("", |m| m.as_str());
      let (check, severity) = match check.strip_suffix(",-warnings-as-errors") {
        Some(original) => (original, "warning"),
        None => (check, &c["severity"]),
      };
      found.push(Self {
        path: base.join(&c["path"]),
        line: c["line"].parse().unwrap_or_default(),
        column: c["column"].parse().unwrap_or_default(),
        severity: severity.to_owned(),
        message: c["message"].to_owned(),
        check: check.to_owned(),
        details: Vec::new(),
      });
      open = true;
    }
    found
  }

  pub fn is_error(&self) -> bool {
    self.severity != "warning"
  }

  fn key(&self) -> (&std::path::Path, u32, u32, &str, &str, &str) {
    (
      &self.path,
      self.line,
      self.column,
      &self.check,
      &self.severity,
      &self.message,
    )
  }
}

impl PartialEq for Diagnostic {
  fn eq(&self, other: &Self) -> bool {
    self.key() == other.key()
  }
}

impl Eq for Diagnostic {}

impl PartialOrd for Diagnostic {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for Diagnostic {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    self.key().cmp(&other.key())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_findings_with_their_details() {
    let output = "2 warnings generated.\nsrc/main.cpp:3:43: error: use nullptr [modernize-use-nullptr,-warnings-as-errors]\n    3 | int* p = 0;\n      |          ^\n/abs/a.h:1:1: warning: bad header [readability-x]\n/abs/a.h:1:1: note: declared here\nerror: unknown argument: '-x' [clang-diagnostic-error]\n";
    let found = Diagnostic::parse_all(output, std::path::Path::new("/root"));
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].path, std::path::Path::new("/root/src/main.cpp"));
    assert_eq!((found[0].line, found[0].column), (3, 43));
    assert_eq!(found[0].severity, "warning");
    assert_eq!(found[0].check, "modernize-use-nullptr");
    assert_eq!(found[0].details.len(), 2);
    assert_eq!(found[1].path, std::path::Path::new("/abs/a.h"));
    assert_eq!(found[1].details.len(), 2);
    assert!(Diagnostic::parse_all("error: unknown argument", std::path::Path::new("/")).is_empty());
  }
}
