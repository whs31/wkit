mod version_file;
mod version_file_kind;
mod version_pattern;

pub use self::{
  version_file::VersionFile, version_file_kind::VersionFileKind, version_pattern::VersionPattern,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VersionSection {
  #[serde(default)]
  pub scheme: wkit_core::version::Scheme,
  pub source: Option<std::path::PathBuf>,
  pub files: Vec<VersionFile>,
}

impl VersionSection {
  pub fn source_file(&self) -> Option<&VersionFile> {
    match &self.source {
      Some(source) => self.files.iter().find(|f| f.path == *source),
      None => self.files.first(),
    }
  }
}
