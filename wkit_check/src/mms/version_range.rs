#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRange {
  lower: Option<Bound>,
  upper: Option<Bound>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Bound {
  version: semver::Version,
  inclusive: bool,
}

impl VersionRange {
  pub fn parse(text: &str) -> Result<Self, String> {
    static COMPARATOR: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(
        r"^(?<op>>=|<=|>|<|=|\^|~)?\s*(?<major>\d+)(?:\.(?<minor>\d+)(?:\.(?<patch>\d+)(?:-(?<pre>[0-9A-Za-z.-]+))?)?)?$",
      )
      .expect("valid")
    });
    static TOKEN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
      regex::Regex::new(r"(?:>=|<=|>|<|=|\^|~)?\s*[^\s,<>=^~]+").expect("valid")
    });
    let mut range = Self {
      lower: None,
      upper: None,
    };
    if text.trim() == "*" {
      return Ok(range);
    }
    let tokens: Vec<_> = TOKEN.find_iter(text).map(|m| m.as_str()).collect();
    let leftover = TOKEN.replace_all(text, "");
    if tokens.is_empty() || leftover.chars().any(|c| !c.is_whitespace() && c != ',') {
      return Err(format!("`{text}` is not a version range"));
    }
    for token in tokens {
      let c = COMPARATOR
        .captures(token)
        .ok_or_else(|| format!("`{token}` is not a version comparator"))?;
      let number = |name: &str| c.name(name).map(|m| m.as_str().parse::<u64>());
      let depth =
        1 + usize::from(c.name("minor").is_some()) + usize::from(c.name("patch").is_some());
      let parse = |r: Option<Result<u64, _>>| {
        r.transpose()
          .map_err(|_| format!("`{token}` is out of range"))
      };
      let mut version = semver::Version::new(
        parse(number("major"))?.unwrap_or_default(),
        parse(number("minor"))?.unwrap_or_default(),
        parse(number("patch"))?.unwrap_or_default(),
      );
      if let Some(pre) = c.name("pre") {
        version.pre =
          semver::Prerelease::new(pre.as_str()).map_err(|e| format!("`{token}`: {e}"))?;
      }
      let partial = depth < 3;
      let next = |depth: usize| Self::next(&version, depth);
      match c.name("op").map_or("=", |m| m.as_str()) {
        ">=" => range.raise(Bound::inclusive(version.clone())),
        ">" if partial => range.raise(Bound::inclusive(next(depth))),
        ">" => range.raise(Bound::exclusive(version.clone())),
        "<" => range.lower_to(Bound::exclusive(version.clone())),
        "<=" if partial => range.lower_to(Bound::exclusive(next(depth))),
        "<=" => range.lower_to(Bound::inclusive(version.clone())),
        "^" => {
          let depth = if version.major > 0 || depth == 1 {
            1
          } else if version.minor > 0 || depth == 2 {
            2
          } else {
            3
          };
          range.raise(Bound::inclusive(version.clone()));
          range.lower_to(Bound::exclusive(next(depth)));
        }
        "~" => {
          range.raise(Bound::inclusive(version.clone()));
          range.lower_to(Bound::exclusive(next(depth.min(2))));
        }
        _ if partial => {
          range.raise(Bound::inclusive(version.clone()));
          range.lower_to(Bound::exclusive(next(depth)));
        }
        _ => {
          range.raise(Bound::inclusive(version.clone()));
          range.lower_to(Bound::inclusive(version.clone()));
        }
      }
    }
    Ok(range)
  }

  pub fn is_satisfiable(&self) -> bool {
    match (&self.lower, &self.upper) {
      (Some(lower), Some(upper)) => match lower.version.cmp(&upper.version) {
        std::cmp::Ordering::Less => true,
        std::cmp::Ordering::Equal => lower.inclusive && upper.inclusive,
        std::cmp::Ordering::Greater => false,
      },
      _ => true,
    }
  }

  fn raise(&mut self, bound: Bound) {
    let stricter = self.lower.as_ref().is_none_or(|current| {
      bound.version > current.version || (bound.version == current.version && !bound.inclusive)
    });
    if stricter {
      self.lower = Some(bound);
    }
  }

  fn lower_to(&mut self, bound: Bound) {
    let stricter = self.upper.as_ref().is_none_or(|current| {
      bound.version < current.version || (bound.version == current.version && !bound.inclusive)
    });
    if stricter {
      self.upper = Some(bound);
    }
  }

  fn next(version: &semver::Version, depth: usize) -> semver::Version {
    match depth {
      1 => semver::Version::new(version.major + 1, 0, 0),
      2 => semver::Version::new(version.major, version.minor + 1, 0),
      _ => semver::Version::new(version.major, version.minor, version.patch + 1),
    }
  }
}

impl Bound {
  fn inclusive(version: semver::Version) -> Self {
    Self {
      version,
      inclusive: true,
    }
  }

  fn exclusive(version: semver::Version) -> Self {
    Self {
      version,
      inclusive: false,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn satisfiable(text: &str) -> bool {
    VersionRange::parse(text).unwrap().is_satisfiable()
  }

  #[test]
  fn ranges_used_by_plugins_are_accepted() {
    for text in [
      ">=10.2 <11",
      ">=3.12.0",
      "^1.4",
      "~2.1.3",
      "1.2",
      "=1.0.0-rc.1",
      "*",
      ">=1.0, <2",
    ] {
      assert!(satisfiable(text), "{text}");
    }
  }

  #[test]
  fn contradictory_ranges_are_empty() {
    for text in [
      ">=11 <10.2",
      ">2 <=2",
      ">=1.5 <1.5",
      "^1.4 >=2",
      ">1.2 <1.3",
    ] {
      assert!(!satisfiable(text), "{text}");
    }
    assert!(satisfiable(">=1.5 <=1.5"));
  }

  #[test]
  fn malformed_ranges_are_rejected() {
    for text in ["", ">=", "=> 1.0", "latest", ">=1.x", "1.2.3.4"] {
      assert!(VersionRange::parse(text).is_err(), "{text}");
    }
  }
}
