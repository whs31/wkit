use wkit_core::{Shell, detect::BuildSystem};
use wkit_manifest::{BuildType, Project};

use crate::{BuildOptions, BuildOutcome, CMake, Cargo, Conan, Error, Result};

#[derive(Debug, Clone, Copy)]
pub struct Builder<'a> {
  project: &'a Project,
  options: &'a BuildOptions,
}

impl<'a> Builder<'a> {
  pub fn new(project: &'a Project, options: &'a BuildOptions) -> Self {
    Self { project, options }
  }

  pub fn system(&self) -> Option<BuildSystem> {
    self
      .project
      .manifest
      .build
      .system
      .or_else(|| BuildSystem::from_path(&self.project.root))
      .or_else(|| Conan::has_workspace(&self.project.root).then_some(BuildSystem::Conan))
  }

  pub fn builds_native(&self) -> bool {
    matches!(self.system(), Some(BuildSystem::Conan | BuildSystem::CMake))
  }

  pub fn build_type(&self) -> Result<BuildType> {
    match self.system() {
      Some(BuildSystem::Conan) => Conan::new(self.project, self.options).build_type(),
      Some(BuildSystem::CMake) => Ok(CMake::new(self.project, self.options).build_type()),
      Some(BuildSystem::Cargo) | None => Ok(self.options.build_type.unwrap_or_default()),
    }
  }

  pub fn run(&self, shell: &mut Shell) -> Result<BuildOutcome> {
    let system = self.system();
    let cargo = Cargo::applies(self.project);
    if system.is_none() && !cargo {
      return Err(Error::Message(
        "nothing to build: no conanfile.py, Cargo.toml or CMakeLists.txt".to_owned(),
      ));
    }
    let outcome = match self.run_native(shell)? {
      Some(outcome) => outcome,
      None => BuildOutcome {
        build_type: self.build_type()?,
        packages: Vec::new(),
      },
    };
    if cargo {
      let primary = system == Some(BuildSystem::Cargo);
      Cargo::new(self.project, self.options).run(outcome.build_type, primary)?;
    }
    Ok(outcome)
  }

  pub fn run_native(&self, shell: &mut Shell) -> Result<Option<BuildOutcome>> {
    let start = std::time::Instant::now();
    let build_type = self.build_type()?;
    let packages = match self.system() {
      Some(BuildSystem::Conan) => Conan::new(self.project, self.options).run(shell, build_type)?,
      Some(BuildSystem::CMake) => CMake::new(self.project, self.options).run(shell, build_type)?,
      Some(BuildSystem::Cargo) | None => return Ok(None),
    };
    shell.finished(format_args!("`{build_type}` build"), start.elapsed())?;
    Ok(Some(BuildOutcome {
      build_type,
      packages,
    }))
  }
}
