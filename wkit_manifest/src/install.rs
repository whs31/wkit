use crate::FileMapping;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct InstallSection {
  pub prefix: Option<std::path::PathBuf>,
  pub files: Vec<FileMapping>,
  pub tmpfiles: Vec<std::path::PathBuf>,
  pub services: Vec<std::path::PathBuf>,
  pub remove: Vec<std::path::PathBuf>,
}

impl InstallSection {
  pub fn is_empty(&self) -> bool {
    self.prefix.is_none()
      && self.files.is_empty()
      && self.tmpfiles.is_empty()
      && self.services.is_empty()
  }
}
