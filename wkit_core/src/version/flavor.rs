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
    match self {
      Self::Full => v.to_string(),
      Self::Numeric => format!("{}.{}.{}", v.major, v.minor, v.patch),
      Self::PkgVer | Self::Debian if v.pre.is_empty() => Self::Numeric.render_semver(v),
      Self::Debian => format!("{}~{}", Self::Numeric.render_semver(v), v.pre),
      Self::PkgVer => {
        let core = Self::Numeric.render_semver(v);
        let pre: String = v.pre.as_str().chars().filter(|&c| c != '-').collect();
        if pre.starts_with(|c: char| c.is_ascii_digit()) {
          format!("{core}pre{pre}")
        } else {
          format!("{core}{pre}")
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn render(flavor: Flavor, v: &str) -> String {
    flavor.render_semver(&semver::Version::parse(v).unwrap())
  }

  #[test]
  fn pkgver_has_no_dashes_and_sorts_before_release() {
    assert_eq!(render(Flavor::PkgVer, "1.2.3"), "1.2.3");
    assert_eq!(render(Flavor::PkgVer, "1.2.3-rc.1"), "1.2.3rc.1");
    assert_eq!(render(Flavor::PkgVer, "1.2.3-alpha-2"), "1.2.3alpha2");
    assert_eq!(render(Flavor::PkgVer, "1.2.3-1"), "1.2.3pre1");
    assert_eq!(render(Flavor::PkgVer, "1.2.3-beta+build.7"), "1.2.3beta");
  }

  #[test]
  fn debian_sorts_prereleases_first_with_a_tilde() {
    assert_eq!(render(Flavor::Debian, "1.2.3"), "1.2.3");
    assert_eq!(render(Flavor::Debian, "1.2.3-rc.1+build.7"), "1.2.3~rc.1");
  }

  #[test]
  fn numeric_drops_prerelease_and_build() {
    assert_eq!(render(Flavor::Numeric, "4.5.6-rc.1+x"), "4.5.6");
  }

  #[test]
  fn full_keeps_everything() {
    assert_eq!(
      render(Flavor::Full, "1.0.0-alpha.1+exp.sha.5114f85"),
      "1.0.0-alpha.1+exp.sha.5114f85"
    );
  }
}
