mod build;
mod check;
mod package;
mod task;
#[cfg(test)]
mod tests;
mod tools;
mod validate;
mod version;

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context as _;

pub use self::{build::*, check::*, package::*, task::*, tools::*, version::*};
use crate::{Language, Result};

pub const MANIFEST_FILE: &str = "wkit.toml";

fn strings(items: &[&str]) -> Vec<String> {
  items.iter().map(|&s| s.to_owned()).collect()
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
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
  #[serde(default)]
  pub conan: ConanSection,
  #[serde(default, rename = "bin")]
  pub bins: Vec<Bin>,
  #[serde(default)]
  pub test: TestSection,
  #[serde(default)]
  pub clean: CleanSection,
  #[serde(default)]
  pub package: PackageSection,
  #[serde(default)]
  pub install: InstallSection,
  #[serde(default)]
  pub scripts: BTreeMap<String, Task>,
  #[serde(skip)]
  pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProjectSection {
  pub name: Option<String>,
  pub description: Option<String>,
  pub languages: BTreeSet<Language>,
  pub default_run: Option<String>,
  pub wkit: Option<semver::VersionReq>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CleanSection {
  pub paths: Option<Vec<String>>,
  pub extra: Vec<String>,
}

impl CleanSection {
  const DEFAULT_PATHS: &[&str] = &[
    "build",
    "cmake-build-*",
    "cmake-visual-studio-*",
    ".cache",
    "graph.html",
    "docs/graph.html",
    "compile_commands.json",
    "CMakeUserPresets.json",
    "conan_provider.cmake",
  ];

  pub fn effective_paths(&self, languages: &BTreeSet<Language>) -> Vec<String> {
    let mut paths = self.paths.clone().unwrap_or_else(|| {
      let mut defaults = strings(Self::DEFAULT_PATHS);
      if languages.contains(&Language::Rust) {
        defaults.push("target".to_owned());
      }
      defaults
    });
    paths.extend(self.extra.iter().cloned());
    paths
  }
}

impl Manifest {
  pub fn parse(source: &str) -> Result<Self> {
    Self::parse_at(source, None)
  }

  pub fn load(path: &std::path::Path) -> Result<Self> {
    Self::parse_at(&fs_err::read_to_string(path)?, Some(path))
  }

  fn parse_at(source: &str, path: Option<&std::path::Path>) -> Result<Self> {
    let location = path
      .map(|p| format!(" at `{}`", p.display()))
      .unwrap_or_default();
    let mut unused = Vec::new();
    let mut manifest: Self = toml::Deserializer::parse(source)
      .and_then(|deserializer| {
        serde_ignored::deserialize(deserializer, |key| unused.push(key.to_string()))
      })
      .map_err(|mut error| {
        error.set_input(Some(source));
        error
      })
      .with_context(|| format!("failed to parse manifest{location}"))?;
    let problems = validate::problems(&manifest);
    anyhow::ensure!(
      problems.is_empty(),
      "invalid manifest{location}:\n  - {}",
      problems.join("\n  - ")
    );
    manifest.warnings = unused
      .into_iter()
      .map(|key| format!("unused manifest key: {key}"))
      .collect();
    Ok(manifest)
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
  pub root: std::path::PathBuf,
  pub manifest_path: std::path::PathBuf,
  pub manifest: Manifest,
}

impl Project {
  pub fn discover(start: &std::path::Path) -> Result<Option<Self>> {
    let start = std::path::absolute(start).unwrap_or_else(|_| start.to_owned());
    crate::fs::find_upwards(&start, MANIFEST_FILE)
      .map(|path| Self::load(&path))
      .transpose()
  }

  pub fn load(manifest_path: &std::path::Path) -> Result<Self> {
    let manifest_path =
      std::path::absolute(manifest_path).unwrap_or_else(|_| manifest_path.to_owned());
    let manifest = Manifest::load(&manifest_path)?;
    let root = manifest_path
      .parent()
      .map(std::path::Path::to_owned)
      .unwrap_or_default();
    Ok(Self {
      root,
      manifest_path,
      manifest,
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

  pub fn tool(&self, name: &str) -> crate::tool::Tool {
    self.manifest.tools.resolve(name, &self.root)
  }

  pub fn ensure_compatible(&self, current: &semver::Version) -> Result<()> {
    if let Some(required) = &self.manifest.project.wkit {
      anyhow::ensure!(
        required.matches(current),
        "this project requires wkit {required}, but wkit {current} is running; run `wkit self-update`"
      );
    }
    Ok(())
  }
}
