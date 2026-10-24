mod parsed;
mod validator;

pub use self::parsed::Parsed;
use self::validator::Validator;
use crate::{
  Bin, BuildSection, CheckSection, CleanSection, FormatSection, ManifestError, ProjectSection,
  Task, TestSection, Tools, VersionSection,
};

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

  #[serde(default, rename = "bin")]
  pub bins: Vec<Bin>,

  #[serde(default)]
  pub test: TestSection,

  #[serde(default)]
  pub clean: CleanSection,

  #[serde(default)]
  pub scripts: std::collections::BTreeMap<String, Task>,
}

impl Manifest {
  pub fn parse(source: &str) -> Result<Parsed, ManifestError> {
    Self::parse_at(source, None)
  }

  pub fn load(path: &std::path::Path) -> Result<Parsed, ManifestError> {
    let source = std::fs::read_to_string(path).map_err(|source| ManifestError::Read {
      path: path.to_owned(),
      source,
    })?;
    Self::parse_at(&source, Some(path))
  }

  fn parse_at(source: &str, path: Option<&std::path::Path>) -> Result<Parsed, ManifestError> {
    let parse_error = |mut error: toml::de::Error| {
      error.set_input(Some(source));
      ManifestError::Parse {
        path: path.map(std::path::Path::to_owned),
        source: Box::new(error),
      }
    };
    let deserializer = toml::Deserializer::parse(source).map_err(parse_error)?;
    let mut unused = Vec::new();
    let manifest: Self =
      serde_ignored::deserialize(deserializer, |key| unused.push(key.to_string()))
        .map_err(parse_error)?;

    let problems = Validator::run(&manifest);
    if !problems.is_empty() {
      return Err(ManifestError::Invalid {
        path: path.map(std::path::Path::to_owned),
        problems,
      });
    }
    let warnings = unused
      .into_iter()
      .map(|key| format!("unused manifest key: {key}"))
      .collect();
    Ok(Parsed { manifest, warnings })
  }

  pub fn languages(&self) -> &std::collections::BTreeSet<wkit_core::Language> {
    &self.project.languages
  }

  pub fn has(&self, language: wkit_core::Language) -> bool {
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
      .unwrap_or_else(|| self.has(wkit_core::Language::Rust))
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
      .unwrap_or_else(|| self.has(wkit_core::Language::Rust))
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
