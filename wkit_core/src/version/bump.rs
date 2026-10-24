use super::VersionError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bump {
  Major,
  Minor,
  Patch,
}

impl Bump {
  pub fn apply(self, v: &semver::Version) -> semver::Version {
    let pre = !v.pre.is_empty();
    let (major, minor, patch) = match self {
      Self::Major if pre && v.minor == 0 && v.patch == 0 => (v.major, 0, 0),
      Self::Major => (v.major + 1, 0, 0),
      Self::Minor if pre && v.patch == 0 => (v.major, v.minor, 0),
      Self::Minor => (v.major, v.minor + 1, 0),
      Self::Patch if pre => (v.major, v.minor, v.patch),
      Self::Patch => (v.major, v.minor, v.patch + 1),
    };
    semver::Version::new(major, minor, patch)
  }

  pub fn apply_pre(self, v: &semver::Version, id: &str) -> Result<semver::Version, VersionError> {
    let invalid = |reason: &str| VersionError::InvalidPrerelease {
      id: id.to_owned(),
      reason: reason.to_owned(),
    };
    if id
      .rsplit('.')
      .next()
      .is_some_and(|last| last.bytes().all(|b| b.is_ascii_digit()))
    {
      return Err(invalid(
        "it must not end with a number; wkit appends the counter",
      ));
    }
    let mut next = self.apply(v);
    let counter = if (next.major, next.minor, next.patch) == (v.major, v.minor, v.patch) {
      match v.pre.as_str().rsplit_once('.') {
        Some((current, n)) if current == id => n.parse::<u64>().map_or(1, |n| n + 1),
        _ => 1,
      }
    } else {
      1
    };
    next.pre =
      semver::Prerelease::new(&format!("{id}.{counter}")).map_err(|e| invalid(&e.to_string()))?;
    if next <= *v {
      return Err(VersionError::NotIncreasing {
        from: v.to_string(),
        to: next.to_string(),
      });
    }
    Ok(next)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn bump(v: &str, b: Bump) -> String {
    b.apply(&semver::Version::parse(v).unwrap()).to_string()
  }

  #[test]
  fn releases_increment_and_reset() {
    assert_eq!(bump("1.2.3", Bump::Patch), "1.2.4");
    assert_eq!(bump("1.2.3", Bump::Minor), "1.3.0");
    assert_eq!(bump("1.2.3", Bump::Major), "2.0.0");
    assert_eq!(bump("1.2.3+build.5", Bump::Patch), "1.2.4");
  }

  #[test]
  fn prereleases_are_released() {
    assert_eq!(bump("1.2.3-rc.1", Bump::Patch), "1.2.3");
    assert_eq!(bump("1.3.0-rc.1", Bump::Minor), "1.3.0");
    assert_eq!(bump("1.2.3-rc.1", Bump::Minor), "1.3.0");
    assert_eq!(bump("2.0.0-alpha", Bump::Major), "2.0.0");
    assert_eq!(bump("2.1.0-alpha", Bump::Major), "3.0.0");
  }

  fn pre(v: &str, b: Bump, id: &str) -> Result<String, VersionError> {
    b.apply_pre(&semver::Version::parse(v).unwrap(), id)
      .map(|v| v.to_string())
  }

  #[test]
  fn prereleases_start_and_advance() {
    assert_eq!(pre("1.2.3", Bump::Patch, "rc").unwrap(), "1.2.4-rc.1");
    assert_eq!(pre("1.2.4-rc.1", Bump::Patch, "rc").unwrap(), "1.2.4-rc.2");
    assert_eq!(pre("1.2.3", Bump::Minor, "rc").unwrap(), "1.3.0-rc.1");
    assert_eq!(pre("1.3.0-rc.1", Bump::Minor, "rc").unwrap(), "1.3.0-rc.2");
    assert_eq!(
      pre("1.2.4-alpha.3", Bump::Patch, "beta").unwrap(),
      "1.2.4-beta.1"
    );
    assert_eq!(pre("1.2.4-rc", Bump::Patch, "rc").unwrap(), "1.2.4-rc.1");
    assert_eq!(pre("1.2.4-rc.1", Bump::Minor, "rc").unwrap(), "1.3.0-rc.1");
  }

  #[test]
  fn prereleases_never_go_backwards() {
    assert!(matches!(
      pre("1.2.4-rc.1", Bump::Patch, "beta"),
      Err(VersionError::NotIncreasing { .. })
    ));
    assert!(matches!(
      pre("1.2.3", Bump::Patch, "rc.1"),
      Err(VersionError::InvalidPrerelease { .. })
    ));
    assert!(pre("1.2.3", Bump::Patch, "r c").is_err());
  }
}
