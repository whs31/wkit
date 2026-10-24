mod document;
mod locator;

pub use jiff::civil::Date;

pub use self::document::{Change, Mismatch, VersionDocument, VersionSet};
use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, derive_more::Display)]
#[serde(rename_all = "lowercase")]
#[display(rename_all = "lowercase")]
pub enum Scheme {
  #[default]
  SemVer,
  CalVer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Flavor {
  #[default]
  Full,
  PkgVer,
  Numeric,
  Debian,
}

impl Flavor {
  pub fn render_semver(self, v: &semver::Version) -> String {
    let core = format!("{}.{}.{}", v.major, v.minor, v.patch);
    match self {
      Self::Full => v.to_string(),
      Self::Numeric => core,
      Self::PkgVer | Self::Debian if v.pre.is_empty() => core,
      Self::Debian => format!("{core}~{}", v.pre),
      Self::PkgVer => {
        let pre: String = v.pre.as_str().chars().filter(|&c| c != '-').collect();
        let sep = if pre.starts_with(|c: char| c.is_ascii_digit()) {
          "pre"
        } else {
          ""
        };
        format!("{core}{sep}{pre}")
      }
    }
  }
}

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

  pub fn apply_pre(self, v: &semver::Version, id: &str) -> Result<semver::Version> {
    anyhow::ensure!(
      !id
        .rsplit('.')
        .next()
        .is_some_and(|last| last.bytes().all(|b| b.is_ascii_digit())),
      "`{id}` is not a valid prerelease identifier: it must not end with a number; wkit appends the counter"
    );
    let mut next = self.apply(v);
    let counter = if (next.major, next.minor, next.patch) == (v.major, v.minor, v.patch) {
      match v.pre.as_str().rsplit_once('.') {
        Some((current, n)) if current == id => n.parse::<u64>().map_or(1, |n| n + 1),
        _ => 1,
      }
    } else {
      1
    };
    next.pre = semver::Prerelease::new(&format!("{id}.{counter}"))
      .map_err(|e| anyhow::anyhow!("`{id}` is not a valid prerelease identifier: {e}"))?;
    anyhow::ensure!(next > *v, "{v} -> {next} would not increase the version");
    Ok(next)
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CalVer {
  pub date: Date,
  pub micro: Option<u32>,
}

impl CalVer {
  #[must_use]
  pub fn next(&self, today: Date) -> Self {
    if self.date < today {
      Self {
        date: today,
        micro: None,
      }
    } else {
      Self {
        date: self.date,
        micro: Some(self.micro.map_or(1, |m| m + 1)),
      }
    }
  }
}

impl std::str::FromStr for CalVer {
  type Err = anyhow::Error;

  fn from_str(input: &str) -> Result<Self> {
    let invalid =
      || anyhow::anyhow!("`{input}` is not a valid calendar version (expected YYYY.MM.DD[.N])");
    let parts: Vec<&str> = input.split('.').collect();
    let widths_ok = matches!(parts.as_slice(), [y, m, d] | [y, m, d, _] if y.len() == 4 && m.len() == 2 && d.len() == 2);
    if !widths_ok
      || parts
        .iter()
        .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
    {
      return Err(invalid());
    }
    let date = Date::new(
      parts[0].parse().map_err(|_| invalid())?,
      parts[1].parse().map_err(|_| invalid())?,
      parts[2].parse().map_err(|_| invalid())?,
    )
    .map_err(|_| invalid())?;
    let micro = match parts.get(3) {
      None => None,
      Some(m) if m.starts_with('0') => return Err(invalid()),
      Some(m) => Some(m.parse().map_err(|_| invalid())?),
    };
    Ok(Self { date, micro })
  }
}

impl std::fmt::Display for CalVer {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.date.strftime("%Y.%m.%d"))?;
    if let Some(micro) = self.micro {
      write!(f, ".{micro}")?;
    }
    Ok(())
  }
}

#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum ProjectVersion {
  Sem(semver::Version),
  Cal(CalVer),
}

impl ProjectVersion {
  pub fn parse(input: &str, scheme: Scheme) -> Result<Self> {
    let input = input.trim();
    match scheme {
      Scheme::SemVer => semver::Version::parse(input)
        .map(Self::Sem)
        .map_err(|e| anyhow::anyhow!("`{input}` is not a valid semantic version: {e}")),
      Scheme::CalVer => input.parse().map(Self::Cal),
    }
  }

  #[must_use]
  pub fn bumped(&self, bump: Bump) -> Self {
    self.bumped_on(bump, jiff::Zoned::now().date())
  }

  #[must_use]
  pub fn bumped_on(&self, bump: Bump, today: Date) -> Self {
    match self {
      Self::Sem(v) => Self::Sem(bump.apply(v)),
      Self::Cal(v) => Self::Cal(v.next(today)),
    }
  }

  pub fn bumped_pre(&self, bump: Bump, id: &str) -> Result<Self> {
    match self {
      Self::Sem(v) => bump.apply_pre(v, id).map(Self::Sem),
      Self::Cal(_) => anyhow::bail!("calendar versions have no prereleases"),
    }
  }

  pub fn render(&self, flavor: Flavor) -> String {
    match self {
      Self::Sem(v) => flavor.render_semver(v),
      Self::Cal(v) => v.to_string(),
    }
  }
}

#[cfg(test)]
mod tests {
  use jiff::civil::date;

  use super::*;

  fn render(flavor: Flavor, v: &str) -> String {
    flavor.render_semver(&semver::Version::parse(v).unwrap())
  }

  #[test]
  fn flavors_render_for_their_package_formats() {
    assert_eq!(render(Flavor::PkgVer, "1.2.3"), "1.2.3");
    assert_eq!(render(Flavor::PkgVer, "1.2.3-rc.1"), "1.2.3rc.1");
    assert_eq!(render(Flavor::PkgVer, "1.2.3-alpha-2"), "1.2.3alpha2");
    assert_eq!(render(Flavor::PkgVer, "1.2.3-1"), "1.2.3pre1");
    assert_eq!(render(Flavor::PkgVer, "1.2.3-beta+build.7"), "1.2.3beta");
    assert_eq!(render(Flavor::Debian, "1.2.3"), "1.2.3");
    assert_eq!(render(Flavor::Debian, "1.2.3-rc.1+build.7"), "1.2.3~rc.1");
    assert_eq!(render(Flavor::Numeric, "4.5.6-rc.1+x"), "4.5.6");
    assert_eq!(
      render(Flavor::Full, "1.0.0-alpha.1+exp.sha.5114f85"),
      "1.0.0-alpha.1+exp.sha.5114f85"
    );
  }

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

  fn pre(v: &str, b: Bump, id: &str) -> Result<String> {
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
    let err = pre("1.2.4-rc.1", Bump::Patch, "beta").unwrap_err();
    assert!(err.to_string().contains("would not increase"), "{err}");
    let err = pre("1.2.3", Bump::Patch, "rc.1").unwrap_err();
    assert!(
      err.to_string().contains("must not end with a number"),
      "{err}"
    );
    assert!(pre("1.2.3", Bump::Patch, "r c").is_err());
  }

  #[test]
  fn calendar_versions_parse_and_display() {
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
  fn calendar_versions_advance_to_today_or_the_next_micro() {
    let today = date(2026, 10, 24);
    let next = |s: &str| s.parse::<CalVer>().unwrap().next(today).to_string();
    assert_eq!(next("2026.10.03"), "2026.10.24");
    assert_eq!(next("2026.10.24"), "2026.10.24.1");
    assert_eq!(next("2026.10.24.4"), "2026.10.24.5");
    assert_eq!(next("2026.11.01"), "2026.11.01.1");
  }

  fn sem(s: &str) -> ProjectVersion {
    ProjectVersion::parse(s, Scheme::SemVer).unwrap()
  }

  #[test]
  fn semver_accepts_full_spec_only() {
    assert_eq!(sem(" 1.0.0-alpha.1+exp ").to_string(), "1.0.0-alpha.1+exp");
    assert!(ProjectVersion::parse("1.0", Scheme::SemVer).is_err());
    assert!(ProjectVersion::parse("01.0.0", Scheme::SemVer).is_err());
  }

  #[test]
  fn project_versions_bump_and_render_through_the_scheme() {
    assert_eq!(sem("1.2.3-rc.1").bumped(Bump::Minor).to_string(), "1.3.0");
    assert_eq!(sem("1.2.3-rc.1").render(Flavor::PkgVer), "1.2.3rc.1");
    let cal = ProjectVersion::parse("2026.10.24", Scheme::CalVer).unwrap();
    assert_eq!(cal.render(Flavor::Numeric), "2026.10.24");
    assert_eq!(
      cal.bumped_on(Bump::Major, date(2026, 10, 24)).to_string(),
      "2026.10.24.1"
    );
    assert!(cal.bumped_pre(Bump::Patch, "rc").is_err());
    assert_eq!(
      sem("1.2.3")
        .bumped_pre(Bump::Patch, "rc")
        .unwrap()
        .to_string(),
      "1.2.4-rc.1"
    );
  }
}
