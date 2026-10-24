use wkit_core::{Shell, detect::BuildSystem};
use wkit_manifest::{BuildType, Project};

use crate::{
  BuildOptions, BuildOutcome, BuiltPackage, CMake, Cargo, CompileCommands, Conan, ConanWorkspace,
  Error, Result,
};

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
      .or_else(|| ConanWorkspace::exists(&self.project.root).then_some(BuildSystem::Conan))
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
    self.native(shell, "build", |system, shell, build_type| match system {
      BuildSystem::Conan => Conan::new(self.project, self.options).run(shell, build_type),
      _ => CMake::new(self.project, self.options).run(shell, build_type),
    })
  }

  pub fn configure(&self, shell: &mut Shell) -> Result<Option<BuildOutcome>> {
    self.native(
      shell,
      "configure",
      |system, shell, build_type| match system {
        BuildSystem::Conan => Conan::new(self.project, self.options).configure(shell, build_type),
        _ => CMake::new(self.project, self.options).configure(shell, build_type),
      },
    )
  }

  pub fn create(&self, shell: &mut Shell) -> Result<BuildType> {
    if self.system() != Some(BuildSystem::Conan) {
      return Err(Error::Message(
        "`wkit create` needs a Conan project (conanfile.py or conanws.yml)".to_owned(),
      ));
    }
    let start = std::time::Instant::now();
    let build_type = self.build_type()?;
    Conan::new(self.project, self.options).create(shell, build_type)?;
    shell.finished(format_args!("`{build_type}` package"), start.elapsed())?;
    Ok(build_type)
  }

  fn native(
    &self,
    shell: &mut Shell,
    what: &str,
    step: impl FnOnce(BuildSystem, &mut Shell, BuildType) -> Result<Vec<BuiltPackage>>,
  ) -> Result<Option<BuildOutcome>> {
    let Some(system) = self.system().filter(|_| self.builds_native()) else {
      return Ok(None);
    };
    let start = std::time::Instant::now();
    let build_type = self.build_type()?;
    let packages = step(system, shell, build_type)?;
    if self.project.manifest.build.link_compile_commands
      && let [package] = packages.as_slice()
      && CompileCommands::link(&self.project.root, &package.build_dir)?
    {
      shell.verbose(|s| s.status("Linked", CompileCommands::FILE))?;
    }
    shell.finished(format_args!("`{build_type}` {what}"), start.elapsed())?;
    Ok(Some(BuildOutcome {
      build_type,
      packages,
    }))
  }
}
