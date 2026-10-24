mod de;
mod validate;

pub mod build;
pub mod check;
pub mod clean;
pub mod error;
pub mod format;
pub mod project;
pub mod task;
pub mod test;
pub mod tools;
pub mod version;

use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Path, PathBuf},
};

use serde::Deserialize;
use wkit_core::{Language, tool::Tool};

pub use crate::{
  build::{Bin, BuildSection, BuildType},
  check::CheckSection,
  clean::CleanSection,
  error::ManifestError,
  format::FormatSection,
  project::ProjectSection,
  task::{Command, CustomCheck, Task},
  test::{TestRunner, TestSection, TestSuite},
  tools::Tools,
  version::{VersionFile, VersionFileKind, VersionSection},
};

pub const MANIFEST_FILE: &str = "wkit.toml";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Manifest {
  pub project: ProjectSection,
  pub version: Option<VersionSection>,
  #[serde(default)]
  pub tools: Tools,
  #[serde(default)]
  pub format: FormatSection,
  #[serde(default)]
  pub check: CheckSection,
  #[serde(default)]
  pub build: BuildSection,
  #[serde(default, rename = "bin")]
  pub bins: Vec<Bin>,
  #[serde(default)]
  pub test: TestSection,
  #[serde(default)]
  pub clean: CleanSection,
  #[serde(default)]
  pub scripts: BTreeMap<String, Task>,
}

#[derive(Debug, Clone)]
pub struct Parsed {
  pub manifest: Manifest,
  pub warnings: Vec<String>,
}

impl Manifest {
  pub fn parse(source: &str) -> Result<Parsed, ManifestError> {
    Self::parse_at(source, None)
  }

  pub fn load(path: &Path) -> Result<Parsed, ManifestError> {
    let source = std::fs::read_to_string(path).map_err(|source| ManifestError::Read {
      path: path.to_owned(),
      source,
    })?;
    Self::parse_at(&source, Some(path))
  }

  fn parse_at(source: &str, path: Option<&Path>) -> Result<Parsed, ManifestError> {
    let parse_error = |mut error: toml::de::Error| {
      error.set_input(Some(source));
      ManifestError::Parse {
        path: path.map(Path::to_owned),
        source: Box::new(error),
      }
    };
    let deserializer = toml::Deserializer::parse(source).map_err(parse_error)?;
    let mut unused = Vec::new();
    let manifest: Self =
      serde_ignored::deserialize(deserializer, |key| unused.push(key.to_string()))
        .map_err(parse_error)?;

    let problems = validate::validate(&manifest);
    if !problems.is_empty() {
      return Err(ManifestError::Invalid {
        path: path.map(Path::to_owned),
        problems,
      });
    }
    let warnings = unused
      .into_iter()
      .map(|key| format!("unused manifest key: {key}"))
      .collect();
    Ok(Parsed { manifest, warnings })
  }

  pub fn languages(&self) -> &BTreeSet<Language> {
    &self.project.languages
  }

  pub fn has(&self, language: Language) -> bool {
    self.project.languages.contains(&language)
  }

  pub fn has_c_family(&self) -> bool {
    self.project.languages.iter().any(|l| l.is_c_family())
  }

  pub fn rust_format_enabled(&self) -> bool {
    self
      .format
      .rust
      .enabled
      .unwrap_or_else(|| self.has(Language::Rust))
  }

  pub fn cpp_format_enabled(&self) -> bool {
    self
      .format
      .cpp
      .enabled
      .unwrap_or_else(|| self.has_c_family())
  }

  pub fn clippy_enabled(&self) -> bool {
    self
      .check
      .clippy
      .enabled
      .unwrap_or_else(|| self.has(Language::Rust))
  }

  pub fn clang_tidy_enabled(&self) -> bool {
    self
      .check
      .clang_tidy
      .enabled
      .unwrap_or_else(|| self.has_c_family())
  }

  pub fn bin(&self, name: &str) -> Option<&Bin> {
    self.bins.iter().find(|b| b.name == name)
  }

  pub fn default_bin(&self) -> Option<&Bin> {
    match &self.project.default_run {
      Some(name) => self.bin(name),
      None if self.bins.len() == 1 => self.bins.first(),
      None => None,
    }
  }
}

#[derive(Debug, Clone)]
pub struct Project {
  pub root: PathBuf,
  pub manifest_path: PathBuf,
  pub manifest: Manifest,
  pub warnings: Vec<String>,
}

impl Project {
  pub fn discover(start: &Path) -> Result<Self, ManifestError> {
    let start = std::path::absolute(start).unwrap_or_else(|_| start.to_owned());
    let path = wkit_core::fs::find_upwards(&start, MANIFEST_FILE)
      .ok_or(ManifestError::NotFound { start })?;
    Self::load(&path)
  }

  pub fn load(manifest_path: &Path) -> Result<Self, ManifestError> {
    let manifest_path =
      std::path::absolute(manifest_path).unwrap_or_else(|_| manifest_path.to_owned());
    let Parsed { manifest, warnings } = Manifest::load(&manifest_path)?;
    let root = manifest_path
      .parent()
      .map(Path::to_owned)
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

  pub fn join(&self, path: impl AsRef<Path>) -> PathBuf {
    self.root.join(path)
  }

  pub fn tool(&self, name: &str) -> Tool {
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
