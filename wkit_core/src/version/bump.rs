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
}
