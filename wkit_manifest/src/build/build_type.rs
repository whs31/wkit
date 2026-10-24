#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, derive_more::Display)]
pub enum BuildType {
  #[default]
  #[serde(alias = "debug")]
  Debug,

  #[serde(alias = "release")]
  Release,

  #[serde(alias = "relwithdebinfo", alias = "rel-with-deb-info")]
  RelWithDebInfo,

  #[serde(alias = "minsizerel", alias = "min-size-rel")]
  MinSizeRel,
}

impl BuildType {
  pub const ALL: [Self; 4] = [
    Self::Debug,
    Self::Release,
    Self::RelWithDebInfo,
    Self::MinSizeRel,
  ];

  pub fn is_optimized(self) -> bool {
    self != Self::Debug
  }
}

impl std::str::FromStr for BuildType {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let wanted = s.replace(['-', '_'], "");
    Self::ALL
      .into_iter()
      .find(|t| t.to_string().eq_ignore_ascii_case(&wanted))
      .ok_or_else(|| {
        format!("unknown build type `{s}` (expected Debug, Release, RelWithDebInfo or MinSizeRel)")
      })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_loosely_and_displays_cmake_names() {
    assert_eq!("release".parse(), Ok(BuildType::Release));
    assert_eq!("rel-with-deb-info".parse(), Ok(BuildType::RelWithDebInfo));
    assert_eq!("MINSIZEREL".parse(), Ok(BuildType::MinSizeRel));
    assert!("fast".parse::<BuildType>().is_err());
    assert_eq!(BuildType::RelWithDebInfo.to_string(), "RelWithDebInfo");
  }
}
