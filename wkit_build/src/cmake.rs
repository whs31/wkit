use wkit_core::{ProcessBuilder, Shell};
use wkit_manifest::{BuildType, Project};

use crate::{BuildOptions, BuiltPackage, Error, PackageRef, Reporter, Result};

#[derive(Debug, Clone, Copy)]
pub struct CMake<'a> {
  project: &'a Project,
  options: &'a BuildOptions,
}

impl<'a> CMake<'a> {
  pub const PRESETS: &'static str = "CMakePresets.json";
  pub const CACHE: &'static str = "CMakeCache.txt";

  pub fn new(project: &'a Project, options: &'a BuildOptions) -> Self {
    Self { project, options }
  }

  pub fn build_type(&self) -> BuildType {
    self
      .options
      .build_type
      .or_else(|| self.preset().and_then(|(_, build_type)| build_type))
      .unwrap_or_default()
  }

  pub fn configure_command(&self, build_type: BuildType) -> ProcessBuilder {
    let mut cmake = ProcessBuilder::new("cmake").cwd(&self.project.root);
    if let Some((name, _)) = self.preset() {
      cmake = cmake.arg("--preset").arg(name);
      if self.options.build_type.is_some() {
        cmake = cmake.arg(format!("-DCMAKE_BUILD_TYPE={build_type}"));
      }
    } else {
      let dir = Self::default_dir(build_type);
      cmake = cmake.arg("-S").arg(".").arg("-B").arg(&dir);
      if !self.project.join(&dir).join(Self::CACHE).is_file() && which::which("ninja").is_ok() {
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

  pub fn build_command(&self, dir: &std::path::Path, build_type: BuildType) -> ProcessBuilder {
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

  pub fn run(&self, shell: &mut Shell, build_type: BuildType) -> Result<Vec<BuiltPackage>> {
    let root = PackageRef::of_project(self.project);
    let log = self.project.join("build").join("wkit-build.log");
    let mut reporter = Reporter::new(shell, root.clone(), build_type, Some(log))?;
    reporter.run(&self.configure_command(build_type))?;
    let mut packages = reporter.finish();
    let Some(built) = packages.pop() else {
      return Err(Error::Message(
        "CMake did not report a build directory".to_owned(),
      ));
    };
    let mut reporter = Reporter::new(shell, root, build_type, None)?;
    reporter.start_build();
    reporter.run(&self.build_command(&built.build_dir, build_type))?;
    Ok(vec![built])
  }

  fn default_dir(build_type: BuildType) -> std::path::PathBuf {
    std::path::Path::new("build").join(build_type.to_string())
  }

  fn preset(&self) -> Option<(String, Option<BuildType>)> {
    let text = std::fs::read_to_string(self.project.join(Self::PRESETS)).ok()?;
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

#[cfg(test)]
mod tests {
  use super::*;

  fn project() -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
      dir.path().join("wkit.toml"),
      "[project]\nlanguages = [\"c\"]\n",
    )
    .unwrap();
    let project = Project::load(&dir.path().join("wkit.toml")).unwrap();
    (dir, project)
  }

  #[test]
  fn presets_decide_the_build_type_unless_overridden() {
    let (dir, project) = project();
    let options = BuildOptions {
      tests: true,
      ..BuildOptions::default()
    };
    let cmake = CMake::new(&project, &options);
    assert_eq!(cmake.build_type(), BuildType::Debug);
    assert!(
      cmake
        .configure_command(BuildType::Debug)
        .to_string()
        .contains("-B build/Debug")
    );

    std::fs::write(
      dir.path().join(CMake::PRESETS),
      r#"{"configurePresets": [{"name": "base", "hidden": true}, {"name": "rel", "cacheVariables": {"CMAKE_BUILD_TYPE": {"type": "STRING", "value": "Release"}}}]}"#,
    )
    .unwrap();
    assert_eq!(cmake.build_type(), BuildType::Release);
    assert_eq!(
      cmake.configure_command(BuildType::Release).to_string(),
      "cmake --preset rel -DCMAKE_EXPORT_COMPILE_COMMANDS=ON -DBUILD_TESTING=ON"
    );
    let options = BuildOptions {
      build_type: Some(BuildType::MinSizeRel),
      jobs: Some(2),
      ..BuildOptions::default()
    };
    let cmake = CMake::new(&project, &options);
    assert_eq!(cmake.build_type(), BuildType::MinSizeRel);
    assert_eq!(
      cmake
        .build_command(std::path::Path::new("build/rel"), BuildType::MinSizeRel)
        .to_string(),
      "cmake --build build/rel --config MinSizeRel -j 2"
    );
  }
}
