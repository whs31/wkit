use wkit_build::{BuildOptions, Builder, Conan};
use wkit_core::{ProcessBuilder, Shell, detect::BuildSystem, version::ProjectVersion};
use wkit_manifest::{BuildType, Project};
use wkit_version::{Mismatch, VersionSet};

use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct Packaging<'a> {
  project: &'a Project,
  options: BuildOptions,
  name: String,
  set: Option<String>,
  output: std::path::PathBuf,
  explicit_output: bool,
}

impl<'a> Packaging<'a> {
  pub const SET_ENV: &'static str = "WKIT_PACKAGE_SET";
  pub const LOG: &'static str = "build/wkit-package.log";

  pub fn new(
    project: &'a Project,
    mut options: BuildOptions,
    set: Option<String>,
    output: Option<std::path::PathBuf>,
  ) -> Result<Self> {
    options.build_type.get_or_insert(BuildType::Release);
    let mut name = Self::base_name(project);
    if let Some(set) = &set {
      let option = project
        .manifest
        .package
        .set_option
        .as_deref()
        .ok_or_else(|| {
          Error::setup(
            format!("cannot package the `{set}` set: no Conan option selects sets"),
            Some("set `package.set-option` in wkit.toml".to_owned()),
          )
        })?;
      if !Self::is_valid_set(set) {
        return Err(Error::setup(
          format!("invalid set `{set}`; use lowercase letters, digits and `_`"),
          None,
        ));
      }
      let default = Conan::new(project, &options).default_option(option)?;
      if default.as_deref() != Some(set) {
        name = format!("{name}-{}", set.replace('_', "-"));
      }
      options.conan_options.push(format!("{option}={set}"));
    }
    let output = output.or_else(|| project.manifest.package.output.clone());
    let explicit_output = output.is_some();
    let output = project.join(output.unwrap_or_else(|| {
      if Self::is_cargo(project) {
        "target/package".into()
      } else {
        "build/package".into()
      }
    }));
    Ok(Self {
      project,
      options,
      name,
      set,
      output,
      explicit_output,
    })
  }

  pub fn project(&self) -> &'a Project {
    self.project
  }

  pub fn options(&self) -> &BuildOptions {
    &self.options
  }

  pub fn name(&self) -> &str {
    &self.name
  }

  pub fn set(&self) -> Option<&str> {
    self.set.as_deref()
  }

  pub fn output(&self) -> &std::path::Path {
    &self.output
  }

  pub fn has_explicit_output(&self) -> bool {
    self.explicit_output
  }

  pub fn build_type(&self) -> BuildType {
    self.options.build_type.unwrap_or(BuildType::Release)
  }

  pub fn is_cargo_project(&self) -> bool {
    Self::is_cargo(self.project)
  }

  pub fn version(&self) -> Result<ProjectVersion> {
    Ok(self.version_set()?.current()?)
  }

  pub fn mismatches(&self) -> Result<Vec<Mismatch>> {
    let set = self.version_set()?;
    Ok(set.mismatches(&set.current()?))
  }

  pub fn stage_dir(&self, format: &str) -> std::path::PathBuf {
    self
      .project
      .join("build/stage")
      .join(format)
      .join(&self.name)
  }

  pub fn shown(&self, path: &std::path::Path) -> String {
    path
      .strip_prefix(&self.project.root)
      .unwrap_or(path)
      .display()
      .to_string()
  }

  pub fn log_path(&self) -> std::path::PathBuf {
    self.project.join(Self::LOG)
  }

  pub fn build_native(&self, shell: &mut Shell) -> Result<std::path::PathBuf> {
    Builder::new(self.project, &self.options)
      .run_native(shell)?
      .and_then(|outcome| outcome.build_dir().map(std::path::Path::to_owned))
      .ok_or_else(|| {
        Error::setup(
          "nothing to package: the project has no CMake build",
          Some(
            "native packages need conanfile.py or CMakeLists.txt with install() rules".to_owned(),
          ),
        )
      })
  }

  pub fn with_env(&self, process: ProcessBuilder) -> ProcessBuilder {
    let process = process
      .env("WKIT_PACKAGE_NAME", &self.name)
      .env("WKIT_BUILD_TYPE", self.build_type().to_string());
    let process = match self.version() {
      Ok(version) => process.env("WKIT_PACKAGE_VERSION", version.to_string()),
      Err(_) => process,
    };
    match &self.set {
      Some(set) => process.env(Self::SET_ENV, set),
      None => process,
    }
  }

  fn version_set(&self) -> Result<VersionSet> {
    let section = self.project.manifest.version.as_ref().ok_or_else(|| {
      Error::setup(
        "packaging needs the project version",
        Some("add a `[version]` section to wkit.toml".to_owned()),
      )
    })?;
    Ok(VersionSet::load(&self.project.root, section)?)
  }

  fn base_name(project: &Project) -> String {
    let manifest = &project.manifest;
    manifest
      .package
      .name
      .clone()
      .or_else(|| {
        Self::is_cargo(project)
          .then(|| manifest.build.cargo.package.clone())
          .flatten()
      })
      .unwrap_or_else(|| project.name())
  }

  fn is_cargo(project: &Project) -> bool {
    let builder_options = BuildOptions::default();
    Builder::new(project, &builder_options).system() == Some(BuildSystem::Cargo)
  }

  fn is_valid_set(set: &str) -> bool {
    set.starts_with(|c: char| c.is_ascii_lowercase())
      && set
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn project(toml: &str) -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("wkit.toml"), toml).unwrap();
    std::fs::write(
      dir.path().join("CMakeLists.txt"),
      "project(x VERSION 1.2.3-rc.1)\n",
    )
    .unwrap();
    let project = Project::load(&dir.path().join("wkit.toml")).unwrap();
    (dir, project)
  }

  #[test]
  fn defaults_to_release_and_the_project_name() {
    let (dir, project) = project(
      "[project]\nname = \"crc8\"\nlanguages = [\"c\"]\n\n[version]\nfiles = [{ path = \"CMakeLists.txt\", pattern = 'VERSION (?<version>\\S+)\\)' }]\n",
    );
    let packaging = Packaging::new(&project, BuildOptions::default(), None, None).unwrap();
    assert_eq!(packaging.name(), "crc8");
    assert_eq!(packaging.build_type(), BuildType::Release);
    assert_eq!(packaging.output(), dir.path().join("build/package"));
    assert_eq!(packaging.version().unwrap().to_string(), "1.2.3-rc.1");
    assert_eq!(
      packaging.stage_dir("deb"),
      dir.path().join("build/stage/deb/crc8")
    );
  }

  #[test]
  fn sets_need_a_conan_option() {
    let (_dir, project) = project("[project]\nlanguages = [\"c\"]\n");
    let err = Packaging::new(
      &project,
      BuildOptions::default(),
      Some("field".into()),
      None,
    )
    .unwrap_err();
    assert!(
      err.to_string().contains("set `package.set-option`"),
      "{err}"
    );
    assert!(
      Packaging::new(&project, BuildOptions::default(), None, None)
        .unwrap()
        .version()
        .unwrap_err()
        .to_string()
        .contains("add a `[version]` section")
    );
    assert!(Packaging::is_valid_set("field_ops2"));
    assert!(!Packaging::is_valid_set("Field") && !Packaging::is_valid_set("2x"));
  }
}
