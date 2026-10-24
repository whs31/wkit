use crate::PackageRef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
  Phase {
    status: &'static str,
    message: String,
  },

  Downloading(PackageRef),

  CompilingDependency(PackageRef),

  Configuring(PackageRef),

  Compiling(PackageRef),

  Testing(PackageRef),

  Step {
    current: u64,
    total: u64,
    target: String,
  },

  Diagnostic(String),

  Warning(String),

  BuildDir(PackageRef, std::path::PathBuf),
}
