mod conan;
mod log;

use std::path::{Path, PathBuf};

use self::log::Reporter;
pub use self::{
  conan::{Conan, ConanWorkspace, RemoteSync},
  log::TEST_PACKAGE,
};
use crate::{
  Language, ProcessBuilder, Result, Shell,
  detect::BuildSystem,
  manifest::{BuildType, Project},
};

const COMPILE_COMMANDS: &str = "compile_commands.json";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildOptions {
  pub build_type: Option<BuildType>,
  pub profile: Option<String>,
  pub tests: bool,
  pub conan_options: Vec<String>,
  pub jobs: Option<usize>,
  pub offline: bool,
  pub extra: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageRef {
  pub name: String,
  pub version: Option<String>,
}

impl PackageRef {
  pub fn new(name: impl Into<String>, version: Option<String>) -> Self {
    Self {
      name: name.into(),
      version,
    }
  }

  pub fn of_project(project: &Project) -> Self {
    let version = project
      .manifest
      .version
      .as_ref()
      .and_then(|section| crate::version::VersionSet::load(&project.root, section).ok())
      .and_then(|set| set.current().ok())
      .map(|version| version.to_string());
    Self::new(project.name(), version)
  }

  pub fn parse(reference: &str) -> Self {
    let reference = reference.split(['#', '@', ':']).next().unwrap_or(reference);
    match reference.split_once('/') {
      Some((name, version)) => Self::new(name, Some(version.to_owned())),
      None => Self::new(reference, None),
    }
  }
}

impl std::fmt::Display for PackageRef {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match &self.version {
      Some(version) => write!(f, "{} v{version}", self.name),
      None => f.write_str(&self.name),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltPackage {
  pub package: PackageRef,
  pub build_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildOutcome {
  pub build_type: BuildType,
  pub packages: Vec<BuiltPackage>,
}

impl BuildOutcome {
  pub fn build_dir(&self) -> Option<&Path> {
    self.packages.last().map(|p| p.build_dir.as_path())
  }
}

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
    let root = &self.project.root;
    self
      .project
      .manifest
      .build
      .system
      .or_else(|| BuildSystem::from_path(root))
      .or_else(|| ConanWorkspace::exists(root).then_some(BuildSystem::Conan))
  }

  pub fn builds_native(&self) -> bool {
    matches!(self.system(), Some(BuildSystem::Conan | BuildSystem::CMake))
  }

  fn conan(&self) -> Conan<'a> {
    Conan::new(self.project, self.options)
  }

  pub fn build_type(&self) -> Result<BuildType> {
    match self.system() {
      Some(BuildSystem::Conan) => self.conan().build_type(),
      Some(BuildSystem::CMake) => Ok(self.cmake_build_type()),
      Some(BuildSystem::Cargo) | None => Ok(self.options.build_type.unwrap_or_default()),
    }
  }

  pub fn run(&self, shell: &mut Shell) -> Result<BuildOutcome> {
    let system = self.system();
    let cargo = self.cargo_applies();
    anyhow::ensure!(
      system.is_some() || cargo,
      "nothing to build: no conanfile.py, Cargo.toml or CMakeLists.txt"
    );
    let outcome = match self.run_native(shell)? {
      Some(outcome) => outcome,
      None => BuildOutcome {
        build_type: self.build_type()?,
        packages: Vec::new(),
      },
    };
    if cargo {
      self.cargo_build(outcome.build_type, system == Some(BuildSystem::Cargo))?;
    }
    Ok(outcome)
  }

  pub fn run_native(&self, shell: &mut Shell) -> Result<Option<BuildOutcome>> {
    self.native(shell, "build", |system, shell, build_type| match system {
      BuildSystem::Conan => self.conan().run(shell, build_type),
      _ => self.cmake(shell, build_type, true),
    })
  }

  pub fn configure(&self, shell: &mut Shell) -> Result<Option<BuildOutcome>> {
    self.native(
      shell,
      "configure",
      |system, shell, build_type| match system {
        BuildSystem::Conan => self.conan().configure(shell, build_type),
        _ => self.cmake(shell, build_type, false),
      },
    )
  }

  pub fn create(&self, shell: &mut Shell) -> Result<BuildType> {
    anyhow::ensure!(
      self.system() == Some(BuildSystem::Conan),
      "`wkit create` needs a Conan project (conanfile.py or conanws.yml)"
    );
    let start = std::time::Instant::now();
    let build_type = self.build_type()?;
    self.conan().create(shell, build_type)?;
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
      && link_compile_commands(&self.project.root, &package.build_dir)?
    {
      shell.verbose(|s| s.status("Linked", COMPILE_COMMANDS))?;
    }
    shell.finished(format_args!("`{build_type}` {what}"), start.elapsed())?;
    Ok(Some(BuildOutcome {
      build_type,
      packages,
    }))
  }

  fn log_path(&self) -> PathBuf {
    self.project.join("build").join("wkit-build.log")
  }

  pub fn cargo_applies(&self) -> bool {
    self.project.manifest.has(Language::Rust) && self.project.join("Cargo.toml").is_file()
  }

  pub fn cargo_command(&self, build_type: BuildType, extra: bool) -> ProcessBuilder {
    let config = &self.project.manifest.build.cargo;
    let mut cargo = config.command(&self.project.root, &["build"]);
    cargo = match &config.package {
      Some(package) => cargo.args(["--package", package]),
      None => cargo.arg("--workspace"),
    };
    if build_type.is_optimized() {
      cargo = cargo.arg("--release");
    }
    if self.options.tests {
      cargo = cargo.arg("--tests");
    }
    if let Some(jobs) = self.options.jobs {
      cargo = cargo.args(["--jobs", &jobs.to_string()]);
    }
    if self.options.offline {
      cargo = cargo.arg("--offline");
    }
    cargo = cargo.args(&config.args);
    if extra {
      cargo = cargo.args(&self.options.extra);
    }
    cargo
  }

  pub fn cargo_run_command(
    &self,
    build_type: BuildType,
    package: Option<&str>,
    bin: Option<&str>,
  ) -> ProcessBuilder {
    let config = &self.project.manifest.build.cargo;
    let mut cargo = config.command(&self.project.root, &["run"]);
    if let Some(package) = package.or(config.package.as_deref()) {
      cargo = cargo.args(["--package", package]);
    }
    if let Some(bin) = bin {
      cargo = cargo.args(["--bin", bin]);
    }
    if build_type.is_optimized() {
      cargo = cargo.arg("--release");
    }
    if self.options.offline {
      cargo = cargo.arg("--offline");
    }
    cargo.args(&config.args).arg("--")
  }

  pub fn cargo_build(&self, build_type: BuildType, extra: bool) -> Result<()> {
    self.cargo_command(build_type, extra).status()
  }

  fn cmake_build_type(&self) -> BuildType {
    self
      .options
      .build_type
      .or_else(|| self.cmake_preset().and_then(|(_, build_type)| build_type))
      .unwrap_or_default()
  }

  fn cmake_configure_command(&self, build_type: BuildType) -> ProcessBuilder {
    let mut cmake = ProcessBuilder::new("cmake").cwd(&self.project.root);
    if let Some((name, _)) = self.cmake_preset() {
      cmake = cmake.arg("--preset").arg(name);
      if self.options.build_type.is_some() {
        cmake = cmake.arg(format!("-DCMAKE_BUILD_TYPE={build_type}"));
      }
    } else {
      let dir = Path::new("build").join(build_type.to_string());
      cmake = cmake.arg("-S").arg(".").arg("-B").arg(&dir);
      if !self.project.join(&dir).join("CMakeCache.txt").is_file() && which::which("ninja").is_ok()
      {
        cmake = cmake.args(["-G", "Ninja"]);
      }
      cmake = cmake.arg(format!("-DCMAKE_BUILD_TYPE={build_type}"));
    }
    cmake = cmake.arg("-DCMAKE_EXPORT_COMPILE_COMMANDS=ON");
    if self.options.tests {
      cmake = cmake.arg("-DBUILD_TESTING=ON");
    }
    cmake
  }

  fn cmake_build_command(&self, dir: &Path, build_type: BuildType) -> ProcessBuilder {
    let mut cmake = ProcessBuilder::new("cmake")
      .cwd(&self.project.root)
      .arg("--build")
      .arg(dir)
      .args(["--config", &build_type.to_string()]);
    if let Some(jobs) = self.options.jobs {
      cmake = cmake.args(["-j", &jobs.to_string()]);
    }
    if !self.options.extra.is_empty() {
      cmake = cmake.arg("--").args(&self.options.extra);
    }
    cmake
  }

  fn cmake(
    &self,
    shell: &mut Shell,
    build_type: BuildType,
    build: bool,
  ) -> Result<Vec<BuiltPackage>> {
    let mut reporter = Reporter::new(
      shell,
      PackageRef::of_project(self.project),
      build_type,
      Some(self.log_path()),
    )?;
    reporter.run(&self.cmake_configure_command(build_type))?;
    let dir = reporter
      .packages()
      .last()
      .map(|p| p.build_dir.clone())
      .ok_or_else(|| anyhow::anyhow!("CMake did not report a build directory"))?;
    if build {
      reporter.start_build();
      reporter.run(&self.cmake_build_command(&dir, build_type))?;
    }
    Ok(reporter.finish())
  }

  fn cmake_preset(&self) -> Option<(String, Option<BuildType>)> {
    let text = std::fs::read_to_string(self.project.join("CMakePresets.json")).ok()?;
    let presets: serde_json::Value = serde_json::from_str(&text).ok()?;
    let preset = presets["configurePresets"]
      .as_array()?
      .iter()
      .find(|p| !p["hidden"].as_bool().unwrap_or(false))?;
    let build_type = &preset["cacheVariables"]["CMAKE_BUILD_TYPE"];
    let build_type = build_type
      .as_str()
      .or_else(|| build_type["value"].as_str())
      .and_then(|t| t.parse().ok());
    Some((preset["name"].as_str()?.to_owned(), build_type))
  }
}

fn link_compile_commands(root: &Path, build_dir: &Path) -> std::io::Result<bool> {
  let target = build_dir.join(COMPILE_COMMANDS);
  let link = root.join(COMPILE_COMMANDS);
  if !target.is_file() || target == link {
    return Ok(false);
  }
  if let Ok(meta) = std::fs::symlink_metadata(&link) {
    if !meta.file_type().is_symlink()
      || std::fs::read_link(&link).is_ok_and(|current| root.join(current) == target)
    {
      return Ok(false);
    }
    std::fs::remove_file(&link)?;
  }
  let relative = target.strip_prefix(root).unwrap_or(&target);
  #[cfg(unix)]
  std::os::unix::fs::symlink(relative, &link)?;
  #[cfg(windows)]
  std::os::windows::fs::symlink_file(relative, &link)?;
  Ok(true)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn project(toml: &str) -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("wkit.toml"), toml).unwrap();
    let project = Project::load(&dir.path().join("wkit.toml")).unwrap();
    (dir, project)
  }

  #[test]
  fn parses_conan_references() {
    assert_eq!(
      PackageRef::parse("zlib/1.3.1#abc:fc02").to_string(),
      "zlib v1.3.1"
    );
    assert_eq!(
      PackageRef::parse("radar-core/0.5.0@user/stable").to_string(),
      "radar-core v0.5.0"
    );
    assert_eq!(PackageRef::parse("cli").to_string(), "cli");
  }

  #[test]
  fn optimized_build_types_build_in_release() {
    let (_dir, project) = project(
      "[project]\nlanguages = [\"rust\"]\n\n[build.cargo]\npackage = \"cli\"\nlocked = true\n",
    );
    let options = BuildOptions {
      offline: true,
      extra: vec!["--timings".into()],
      ..BuildOptions::default()
    };
    let builder = Builder::new(&project, &options);
    assert_eq!(
      builder
        .cargo_command(BuildType::RelWithDebInfo, true)
        .to_string(),
      "cargo build --locked --package cli --release --offline --timings"
    );
    assert_eq!(
      builder.cargo_command(BuildType::Debug, false).to_string(),
      "cargo build --locked --package cli --offline"
    );
    assert_eq!(
      builder
        .cargo_run_command(BuildType::Release, None, Some("encode"))
        .to_string(),
      "cargo run --locked --package cli --bin encode --release --offline --"
    );
  }

  #[test]
  fn cmake_presets_decide_the_build_type_unless_overridden() {
    let (dir, project) = project("[project]\nlanguages = [\"c\"]\n");
    let options = BuildOptions {
      tests: true,
      ..BuildOptions::default()
    };
    let cmake = Builder::new(&project, &options);
    assert_eq!(cmake.cmake_build_type(), BuildType::Debug);
    assert!(
      cmake
        .cmake_configure_command(BuildType::Debug)
        .to_string()
        .contains(&format!(
          "-B {}",
          Path::new("build").join("Debug").display()
        ))
    );

    std::fs::write(
      dir.path().join("CMakePresets.json"),
      r#"{"configurePresets": [{"name": "base", "hidden": true}, {"name": "rel", "cacheVariables": {"CMAKE_BUILD_TYPE": {"type": "STRING", "value": "Release"}}}]}"#,
    )
    .unwrap();
    assert_eq!(cmake.cmake_build_type(), BuildType::Release);
    assert_eq!(
      cmake
        .cmake_configure_command(BuildType::Release)
        .to_string(),
      "cmake --preset rel -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DBUILD_TESTING=ON"
    );
    let options = BuildOptions {
      build_type: Some(BuildType::MinSizeRel),
      jobs: Some(2),
      ..BuildOptions::default()
    };
    let cmake = Builder::new(&project, &options);
    assert_eq!(cmake.cmake_build_type(), BuildType::MinSizeRel);
    assert_eq!(
      cmake
        .cmake_build_command(Path::new("build/rel"), BuildType::MinSizeRel)
        .to_string(),
      "cmake --build build/rel --config MinSizeRel -j 2"
    );
  }

  #[cfg(unix)]
  #[test]
  fn links_the_active_build_and_leaves_real_files_alone() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for build in ["build/Debug", "build/Release"] {
      std::fs::create_dir_all(root.join(build)).unwrap();
      std::fs::write(root.join(build).join(COMPILE_COMMANDS), build).unwrap();
    }
    let read = || std::fs::read_to_string(root.join(COMPILE_COMMANDS)).unwrap();
    assert!(link_compile_commands(root, &root.join("build/Debug")).unwrap());
    assert_eq!(read(), "build/Debug");
    assert!(!link_compile_commands(root, &root.join("build/Debug")).unwrap());
    assert!(link_compile_commands(root, &root.join("build/Release")).unwrap());
    assert_eq!(read(), "build/Release");
    assert_eq!(
      std::fs::read_link(root.join(COMPILE_COMMANDS)).unwrap(),
      Path::new("build/Release/compile_commands.json")
    );

    std::fs::remove_file(root.join(COMPILE_COMMANDS)).unwrap();
    std::fs::write(root.join(COMPILE_COMMANDS), "mine").unwrap();
    assert!(!link_compile_commands(root, &root.join("build/Debug")).unwrap());
    assert_eq!(read(), "mine");
  }
}
