use crate::PackageRef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltPackage {
  pub package: PackageRef,
  pub build_dir: std::path::PathBuf,
}
