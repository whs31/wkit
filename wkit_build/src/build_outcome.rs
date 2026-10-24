use wkit_manifest::BuildType;

use crate::BuiltPackage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildOutcome {
  pub build_type: BuildType,
  pub packages: Vec<BuiltPackage>,
}

impl BuildOutcome {
  pub fn build_dir(&self) -> Option<&std::path::Path> {
    self.packages.last().map(|p| p.build_dir.as_path())
  }
}
