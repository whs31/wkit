use wkit_manifest::{BuildType, Project};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildDir;

impl BuildDir {
  pub fn resolve(
    project: &Project,
    template: Option<&str>,
    build_type: BuildType,
    built: Option<&std::path::Path>,
  ) -> Result<std::path::PathBuf> {
    let Some(template) = template else {
      return built.map(ToOwned::to_owned).ok_or_else(|| {
        Error::setup(
          "the project has no CMake build directory to test in",
          Some("set `build-dir` in `[[test.suite]]`".to_owned()),
        )
      });
    };
    let dir = project.join(template.replace("{build-type}", &build_type.to_string()));
    if dir.is_dir() {
      Ok(dir)
    } else {
      Err(Error::setup(
        format!("build directory `{}` does not exist", dir.display()),
        Some("check `build-dir` in `[[test.suite]]`".to_owned()),
      ))
    }
  }
}
