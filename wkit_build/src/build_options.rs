use wkit_manifest::BuildType;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildOptions {
  pub build_type: Option<BuildType>,
  pub profile: Option<String>,
  pub tests: bool,
  pub conan_options: Vec<String>,
  pub jobs: Option<usize>,
  pub offline: bool,
  pub extra: Vec<String>,
}
