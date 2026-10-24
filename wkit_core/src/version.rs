use std::{fmt, str::FromStr};

pub use jiff::civil::Date;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VersionError {
  #[error("`{input}` is not a valid semantic version: {reason}")]
  InvalidSemver { input: String, reason: String },

  #[error("`{input}` is not a valid calendar version (expected YYYY.MM.DD[.N])")]
  InvalidCalver { input: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum Scheme {
  #[default]
  #[serde(rename = "semver")]
  SemVer,
  #[serde(rename = "calver")]
  CalVer,
}

impl fmt::Display for Scheme {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(match self {
      Self::SemVer => "semver",
      Self::CalVer => "calver",
    })
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Flavor {
  #[default]
  Full,
  #[serde(rename = "pkgver")]
  PkgVer,
  Numeric,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bump {
  Major,
  Minor,
  Patch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectVersion {
  Sem(semver::Version),
  Cal(CalVer),
}

impl ProjectVersion {
  pub fn parse(input: &str, scheme: Scheme) -> Result<Self, VersionError> {
    let input = input.trim();
    match scheme {
      Scheme::SemVer => {
        semver::Version::parse(input)
          .map(Self::Sem)
          .map_err(|e| VersionError::InvalidSemver {
            input: input.to_owned(),
            reason: e.to_string(),
          })
      }
      Scheme::CalVer => input.parse().map(Self::Cal),
    }
  }

  pub fn scheme(&self) -> Scheme {
    match self {
      Self::Sem(_) => Scheme::SemVer,
      Self::Cal(_) => Scheme::CalVer,
    }
  }

  #[must_use]
  pub fn bumped(&self, bump: Bump) -> Self {
    self.bumped_on(bump, jiff::Zoned::now().date())
  }

  #[must_use]
  pub fn bumped_on(&self, bump: Bump, today: Date) -> Self {
    match self {
      Self::Sem(v) => Self::Sem(bump_semver(v, bump)),
      Self::Cal(v) => Self::Cal(v.next(today)),
    }
  }

  pub fn render(&self, flavor: Flavor) -> String {
    match (self, flavor) {
      (Self::Sem(v), Flavor::Full) => v.to_string(),
      (Self::Sem(v), Flavor::PkgVer) => semver_pkgver(v),
      (Self::Sem(v), Flavor::Numeric) => format!("{}.{}.{}", v.major, v.minor, v.patch),
      (Self::Cal(v), _) => v.to_string(),
    }
  }
}

impl fmt::Display for ProjectVersion {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.render(Flavor::Full))
  }
}

fn bump_semver(v: &semver::Version, bump: Bump) -> semver::Version {
  let pre = !v.pre.is_empty();
  let (major, minor, patch) = match bump {
    Bump::Major if pre && v.minor == 0 && v.patch == 0 => (v.major, 0, 0),
    Bump::Major => (v.major + 1, 0, 0),
    Bump::Minor if pre && v.patch == 0 => (v.major, v.minor, 0),
    Bump::Minor => (v.major, v.minor + 1, 0),
    Bump::Patch if pre => (v.major, v.minor, v.patch),
    Bump::Patch => (v.major, v.minor, v.patch + 1),
  };
  semver::Version::new(major, minor, patch)
}

fn semver_pkgver(v: &semver::Version) -> String {
  let core = format!("{}.{}.{}", v.major, v.minor, v.patch);
  if v.pre.is_empty() {
    return core;
  }
  let pre: String = v.pre.as_str().chars().filter(|&c| c != '-').collect();
  if pre.starts_with(|c: char| c.is_ascii_digit()) {
    format!("{core}pre{pre}")
  } else {
    format!("{core}{pre}")
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CalVer {
  pub date: Date,
  pub micro: Option<u32>,
}

impl CalVer {
  pub fn on(date: Date) -> Self {
    Self { date, micro: None }
  }

  #[must_use]
  pub fn next(&self, today: Date) -> Self {
    if self.date < today {
      Self::on(today)
    } else {
      Self {
        date: self.date,
        micro: Some(self.micro.map_or(1, |m| m + 1)),
      }
    }
  }
}

impl FromStr for CalVer {
  type Err = VersionError;

  fn from_str(input: &str) -> Result<Self, Self::Err> {
    let invalid = || VersionError::InvalidCalver {
      input: input.to_owned(),
    };
    let parts: Vec<&str> = input.split('.').collect();
    let widths_ok =
      matches!(parts.as_slice(), [y, m, d, ..] if y.len() == 4 && m.len() == 2 && d.len() == 2);
    if !widths_ok
      || parts.len() > 4
      || parts
        .iter()
        .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
    {
      return Err(invalid());
    }
    let year: i16 = parts[0].parse().map_err(|_| invalid())?;
    let month: i8 = parts[1].parse().map_err(|_| invalid())?;
    let day: i8 = parts[2].parse().map_err(|_| invalid())?;
    let date = Date::new(year, month, day).map_err(|_| invalid())?;
    let micro = match parts.get(3) {
      None => None,
      Some(m) if m.starts_with('0') => return Err(invalid()),
      Some(m) => Some(m.parse().map_err(|_| invalid())?),
    };
    Ok(Self { date, micro })
  }
}

impl fmt::Display for CalVer {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "{:04}.{:02}.{:02}",
      self.date.year(),
      self.date.month(),
      self.date.day()
    )?;
    if let Some(micro) = self.micro {
      write!(f, ".{micro}")?;
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use jiff::civil::date;

  use super::*;

  fn sem(s: &str) -> ProjectVersion {
    ProjectVersion::parse(s, Scheme::SemVer).unwrap()
  }

  fn bump(s: &str, b: Bump) -> String {
    sem(s).bumped(b).to_string()
  }

  #[test]
  fn semver_bumps() {
    assert_eq!(bump("1.2.3", Bump::Patch), "1.2.4");
    assert_eq!(bump("1.2.3", Bump::Minor), "1.3.0");
    assert_eq!(bump("1.2.3", Bump::Major), "2.0.0");
    assert_eq!(bump("1.2.3+build.5", Bump::Patch), "1.2.4");
  }

  #[test]
  fn semver_bumps_release_prereleases() {
    assert_eq!(bump("1.2.3-rc.1", Bump::Patch), "1.2.3");
    assert_eq!(bump("1.3.0-rc.1", Bump::Minor), "1.3.0");
    assert_eq!(bump("1.2.3-rc.1", Bump::Minor), "1.3.0");
    assert_eq!(bump("2.0.0-alpha", Bump::Major), "2.0.0");
    assert_eq!(bump("2.1.0-alpha", Bump::Major), "3.0.0");
  }

  #[test]
  fn semver_accepts_full_spec() {
    let v = sem("1.0.0-alpha.1+exp.sha.5114f85");
    assert_eq!(v.to_string(), "1.0.0-alpha.1+exp.sha.5114f85");
    assert!(ProjectVersion::parse("1.0", Scheme::SemVer).is_err());
    assert!(ProjectVersion::parse("01.0.0", Scheme::SemVer).is_err());
  }

  #[test]
  fn pkgver_has_no_dashes_and_sorts_before_release() {
    assert_eq!(sem("1.2.3").render(Flavor::PkgVer), "1.2.3");
    assert_eq!(sem("1.2.3-rc.1").render(Flavor::PkgVer), "1.2.3rc.1");
    assert_eq!(sem("1.2.3-alpha-2").render(Flavor::PkgVer), "1.2.3alpha2");
    assert_eq!(sem("1.2.3-1").render(Flavor::PkgVer), "1.2.3pre1");
    assert_eq!(
      sem("1.2.3-beta+build.7").render(Flavor::PkgVer),
      "1.2.3beta"
    );
  }

  #[test]
  fn numeric_drops_prerelease_and_build() {
    assert_eq!(sem("4.5.6-rc.1+x").render(Flavor::Numeric), "4.5.6");
  }

  #[test]
  fn calver_parse_and_display() {
    let v: CalVer = "2026.10.03".parse().unwrap();
    assert_eq!(v.to_string(), "2026.10.03");
    let v: CalVer = "2026.10.03.2".parse().unwrap();
    assert_eq!(v.micro, Some(2));
    for bad in [
      "2026.10.3",
      "2026.13.01",
      "2026.02.30",
      "2026.10.03.0",
      "2026.10.03.01",
      "2026.10",
      "26.10.03",
      "2026.10.03.1.1",
    ] {
      assert!(bad.parse::<CalVer>().is_err(), "{bad}");
    }
  }

  #[test]
  fn calver_bumps_to_today_or_next_micro() {
    let today = date(2026, 10, 24);
    let cal = |s: &str| ProjectVersion::parse(s, Scheme::CalVer).unwrap();
    assert_eq!(
      cal("2026.10.03").bumped_on(Bump::Patch, today).to_string(),
      "2026.10.24"
    );
    assert_eq!(
      cal("2026.10.24").bumped_on(Bump::Patch, today).to_string(),
      "2026.10.24.1"
    );
    assert_eq!(
      cal("2026.10.24.4")
        .bumped_on(Bump::Major, today)
        .to_string(),
      "2026.10.24.5"
    );
    assert_eq!(
      cal("2026.11.01").bumped_on(Bump::Patch, today).to_string(),
      "2026.11.01.1"
    );
  }
}
