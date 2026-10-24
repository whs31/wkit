use super::{Bump, CalVer, Date, Flavor, Scheme, VersionError};

#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
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
      Self::Sem(v) => Self::Sem(bump.apply(v)),
      Self::Cal(v) => Self::Cal(v.next(today)),
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
  use super::*;

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
  fn bumps_and_renders_through_the_scheme() {
    assert_eq!(sem("1.2.3-rc.1").bumped(Bump::Minor).to_string(), "1.3.0");
    assert_eq!(sem("1.2.3-rc.1").render(Flavor::PkgVer), "1.2.3rc.1");
    let cal = ProjectVersion::parse("2026.10.24", Scheme::CalVer).unwrap();
    assert_eq!(cal.scheme(), Scheme::CalVer);
    assert_eq!(cal.render(Flavor::Numeric), "2026.10.24");
    let today = jiff::civil::date(2026, 10, 24);
    assert_eq!(
      cal.bumped_on(Bump::Major, today).to_string(),
      "2026.10.24.1"
    );
  }
}
