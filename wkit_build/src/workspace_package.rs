use crate::PackageRef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePackage {
  pub path: std::path::PathBuf,
  pub reference: Option<PackageRef>,
}
