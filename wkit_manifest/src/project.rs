mod project_section;

pub use self::project_section::ProjectSection;
use crate::{MANIFEST_FILE, Manifest, ManifestError, Parsed};

#[derive(Debug, Clone)]
pub struct Project {
  pub root: std::path::PathBuf,
  pub manifest_path: std::path::PathBuf,
  pub manifest: Manifest,
  pub warnings: Vec<String>,
}

impl Project {
  pub fn discover(start: &std::path::Path) -> Result<Self, ManifestError> {
    let start = std::path::absolute(start).unwrap_or_else(|_| start.to_owned());
    let path = wkit_core::fs::find_upwards(&start, MANIFEST_FILE)
      .ok_or(ManifestError::NotFound { start })?;
    Self::load(&path)
  }

  pub fn load(manifest_path: &std::path::Path) -> Result<Self, ManifestError> {
    let manifest_path =
      std::path::absolute(manifest_path).unwrap_or_else(|_| manifest_path.to_owned());
    let Parsed { manifest, warnings } = Manifest::load(&manifest_path)?;
    let root = manifest_path
      .parent()
      .map(std::path::Path::to_owned)
      .unwrap_or_default();
    Ok(Self {
      root,
      manifest_path,
      manifest,
      warnings,
    })
  }

  pub fn name(&self) -> String {
    self.manifest.project.name.clone().unwrap_or_else(|| {
      self.root.file_name().map_or_else(
        || "project".to_owned(),
        |n| n.to_string_lossy().into_owned(),
      )
    })
  }

  pub fn join(&self, path: impl AsRef<std::path::Path>) -> std::path::PathBuf {
    self.root.join(path)
  }

  pub fn tool(&self, name: &str) -> wkit_core::tool::Tool {
    self.manifest.tools.resolve(name, &self.root)
  }

  pub fn ensure_compatible(&self, current: &semver::Version) -> Result<(), ManifestError> {
    match &self.manifest.project.wkit {
      Some(required) if !required.matches(current) => Err(ManifestError::Incompatible {
        required: required.clone(),
        current: current.clone(),
      }),
      _ => Ok(()),
    }
  }
}
