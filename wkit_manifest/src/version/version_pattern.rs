#[derive(Debug, Clone)]
pub struct VersionPattern(regex::Regex);

impl PartialEq for VersionPattern {
  fn eq(&self, other: &Self) -> bool {
    self.0.as_str() == other.0.as_str()
  }
}

impl Eq for VersionPattern {}

impl VersionPattern {
  pub fn new(pattern: &str) -> Result<Self, String> {
    let regex = regex::Regex::new(pattern).map_err(|e| format!("invalid pattern: {e}"))?;
    if regex.captures_len() < 2 {
      return Err(format!(
        "pattern `{pattern}` must capture the version, either with a `(?<version>...)` group or a single unnamed group"
      ));
    }
    Ok(Self(regex))
  }

  pub fn lua_defaults() -> &'static [Self] {
    static LUA: std::sync::LazyLock<[VersionPattern; 2]> = std::sync::LazyLock::new(|| {
      [
        VersionPattern::new(r#"mms\.Version\.parse\(\s*["'](?<version>[^"']+)["']\s*\)"#)
          .expect("valid"),
        VersionPattern::new(r#"(?m)^\s*version\s*=\s*["'](?<version>[^"']+)["']"#).expect("valid"),
      ]
    });
    LUA.as_slice()
  }

  pub fn as_str(&self) -> &str {
    self.0.as_str()
  }

  pub fn find<'h>(&self, haystack: &'h str) -> Option<regex::Match<'h>> {
    let captures = self.0.captures(haystack)?;
    captures.name("version").or_else(|| captures.get(1))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn needs_a_capture_group() {
    let err = VersionPattern::new("version").unwrap_err();
    assert!(err.contains("must capture the version"), "{err}");
    assert!(
      VersionPattern::new("(")
        .unwrap_err()
        .starts_with("invalid pattern")
    );
  }

  #[test]
  fn prefers_the_named_group() {
    let p = VersionPattern::new(r"(v)(?<version>\d+\.\d+)").unwrap();
    assert_eq!(p.find("tag v1.2").unwrap().as_str(), "1.2");
    let p = VersionPattern::new(r"^(\S+)$").unwrap();
    assert_eq!(p.find("3.4.5").unwrap().as_str(), "3.4.5");
  }
}
