mod arch_package;
mod deb_package;
mod file_mapping;
mod file_mode;
mod windows_package;

pub use self::{
  arch_package::ArchPackage,
  deb_package::DebPackage,
  file_mapping::FileMapping,
  file_mode::FileMode,
  windows_package::{WindowsGenerator, WindowsPackage},
};
use crate::Task;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct PackageSection {
  pub name: Option<String>,
  pub set_option: Option<String>,
  pub output: Option<std::path::PathBuf>,
  pub license: Option<std::path::PathBuf>,
  pub exclude: Vec<String>,
  pub files: Vec<FileMapping>,
  pub stage: Option<Task>,
  pub deb: Option<DebPackage>,
  pub arch: ArchPackage,
  pub windows: WindowsPackage,
}

impl PackageSection {
  pub const DEFAULT_EXCLUDE: &[&str] = &["include", "lib/cmake", "lib/pkgconfig"];
}

impl Default for PackageSection {
  fn default() -> Self {
    Self {
      name: None,
      set_option: None,
      output: None,
      license: None,
      exclude: Self::DEFAULT_EXCLUDE
        .iter()
        .map(|&s| s.to_owned())
        .collect(),
      files: Vec::new(),
      stage: None,
      deb: None,
      arch: ArchPackage::default(),
      windows: WindowsPackage::default(),
    }
  }
}
