#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageRef {
  pub name: String,
  pub version: Option<String>,
}

impl PackageRef {
  pub fn new(name: impl Into<String>, version: Option<String>) -> Self {
    Self {
      name: name.into(),
      version,
    }
  }

  pub fn of_project(project: &wkit_manifest::Project) -> Self {
    let version = project
      .manifest
      .version
      .as_ref()
      .and_then(|section| wkit_version::VersionSet::load(&project.root, section).ok())
      .and_then(|set| set.current().ok())
      .map(|version| version.to_string());
    Self::new(project.name(), version)
  }

  pub fn parse(reference: &str) -> Self {
    let reference = reference.split(['#', '@', ':']).next().unwrap_or(reference);
    match reference.split_once('/') {
      Some((name, version)) => Self::new(name, Some(version.to_owned())),
      None => Self::new(reference, None),
    }
  }
}

impl std::fmt::Display for PackageRef {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match &self.version {
      Some(version) => write!(f, "{} v{version}", self.name),
      None => f.write_str(&self.name),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_conan_references() {
    assert_eq!(
      PackageRef::parse("zlib/1.3.1#abc:fc02").to_string(),
      "zlib v1.3.1"
    );
    assert_eq!(
      PackageRef::parse("radar-core/0.5.0@user/stable").to_string(),
      "radar-core v0.5.0"
    );
    assert_eq!(PackageRef::parse("cli").to_string(), "cli");
  }
}
