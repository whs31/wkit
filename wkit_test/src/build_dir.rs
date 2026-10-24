use wkit_core::detect::BuildSystem;
use wkit_manifest::{BuildType, Project};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildDir;

impl BuildDir {
  pub const CTEST_FILE: &str = "CTestTestfile.cmake";

  pub fn resolve(
    project: &Project,
    template: Option<&str>,
    build_type: BuildType,
  ) -> Result<std::path::PathBuf> {
    let expand = |t: &str| project.join(t.replace("{build-type}", &build_type.to_string()));
    if let Some(template) = template {
      let dir = expand(template);
      return if dir.is_dir() {
        Ok(dir)
      } else {
        Err(Self::missing(project, &dir, build_type))
      };
    }
    let preferred = expand("build/{build-type}");
    if preferred.join(Self::CTEST_FILE).is_file() {
      return Ok(preferred);
    }
    let build = project.join("build");
    let mut candidates: Vec<_> = std::fs::read_dir(&build)
      .into_iter()
      .flatten()
      .flatten()
      .map(|e| e.path())
      .chain(std::iter::once(build.clone()))
      .filter_map(|dir| {
        let modified = std::fs::metadata(dir.join(Self::CTEST_FILE))
          .ok()?
          .modified()
          .ok()?;
        Some((modified, dir))
      })
      .collect();
    candidates.sort();
    candidates
      .pop()
      .map(|(_, dir)| dir)
      .ok_or_else(|| Self::missing(project, &preferred, build_type))
  }

  pub fn hint(project: &Project, build_type: BuildType) -> String {
    let system = project
      .manifest
      .build
      .system
      .or_else(|| BuildSystem::from_path(&project.root));
    match system {
      Some(BuildSystem::Conan) => {
        let option = project
          .manifest
          .build
          .conan
          .test_option
          .as_deref()
          .map(|o| format!(" -o \"&:{o}=True\""))
          .unwrap_or_default();
        format!(
          "build with tests first: `conan build .{option} -s build_type={build_type} --build=missing`"
        )
      }
      _ => format!(
        "configure and build with tests first, e.g. `cmake -B build/{build_type} -DCMAKE_BUILD_TYPE={build_type} && cmake --build build/{build_type}`"
      ),
    }
  }

  fn missing(project: &Project, dir: &std::path::Path, build_type: BuildType) -> Error {
    Error::setup(
      format!("build directory `{}` does not exist", dir.display()),
      Some(Self::hint(project, build_type)),
    )
  }
}
