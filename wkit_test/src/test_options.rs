use wkit_manifest::BuildType;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestOptions {
  pub junit: Option<std::path::PathBuf>,
  pub suites: Vec<String>,
  pub build_type: BuildType,
  pub build_dir: Option<std::path::PathBuf>,
  pub extra: Vec<String>,
  pub color: bool,
}
