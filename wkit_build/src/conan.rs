use wkit_core::{ProcessBuilder, Shell};
use wkit_manifest::{BuildType, Project, build::ConanBuild};

use crate::{BuildOptions, BuiltPackage, Error, PackageRef, Reporter, Result};

#[derive(Debug, Clone, Copy)]
pub struct Conan<'a> {
  project: &'a Project,
  options: &'a BuildOptions,
}

impl<'a> Conan<'a> {
  pub const WORKSPACE_FILES: [&'static str; 2] = ["conanws.yml", "conanws.py"];

  pub fn new(project: &'a Project, options: &'a BuildOptions) -> Self {
    Self { project, options }
  }

  pub fn has_workspace(root: &std::path::Path) -> bool {
    Self::WORKSPACE_FILES.iter().any(|f| root.join(f).is_file())
  }

  pub fn is_workspace(&self) -> bool {
    self.config().workspace
      || (!self.project.join("conanfile.py").is_file() && Self::has_workspace(&self.project.root))
  }

  pub fn build_type(&self) -> Result<BuildType> {
    if let Some(build_type) = self.options.build_type {
      return Ok(build_type);
    }
    let profile_name = self.profile().unwrap_or("default");
    let profile = self.query(
      &["profile", "show"],
      &self.profile_args(),
      &format!("could not read the `{profile_name}` profile"),
    )?;
    let value = profile["host"]["settings"]["build_type"]
      .as_str()
      .ok_or_else(|| {
        Error::Message(format!(
          "the `{profile_name}` profile does not set `build_type`; set it there or pass `--build-type`"
        ))
      })?;
    value.parse().map_err(Error::Message)
  }

  pub fn command(&self, build_type: BuildType, test_options: &[String]) -> ProcessBuilder {
    let config = self.config();
    let mut conan = self.conan();
    conan = if self.is_workspace() {
      conan.args(["workspace", "build"])
    } else {
      conan.args(["build", "."])
    };
    conan = conan.args(self.profile_args());
    if self.options.build_type.is_some() {
      conan = conan.arg("-s").arg(format!("build_type={build_type}"));
    }
    conan = conan.args(test_options).args(config.conf_args());
    if let Some(jobs) = self.options.jobs {
      conan = conan.arg("-c").arg(format!("tools.build:jobs={jobs}"));
    }
    if config.build_missing {
      conan = conan.arg("--build=missing");
    }
    if self.options.offline {
      conan = conan.arg("--no-remote");
    }
    conan.args(&config.args).args(&self.options.extra)
  }

  pub fn test_options(&self) -> Result<Vec<String>> {
    if !self.options.tests {
      return Ok(Vec::new());
    }
    let config = self.config();
    let packages = if self.is_workspace() {
      self.workspace_packages()?
    } else {
      vec![(".".to_owned(), None)]
    };
    let mut args = Vec::new();
    for (path, reference) in packages {
      let option = match &config.test_option {
        Some(option) => Some(option.clone()),
        None => self.inferred_test_option(&path)?,
      };
      if let Some(option) = option {
        let scope =
          reference.map_or_else(|| "&".to_owned(), |r: PackageRef| format!("{}/*", r.name));
        args.extend(["-o".to_owned(), format!("{scope}:{option}=True")]);
      }
    }
    Ok(args)
  }

  pub fn run(&self, shell: &mut Shell, build_type: BuildType) -> Result<Vec<BuiltPackage>> {
    let command = self.command(build_type, &self.test_options()?);
    let root = PackageRef::of_project(self.project);
    let mut reporter = Reporter::new(shell, root, build_type, Some(self.log_path()))?;
    reporter.run(&command)?;
    Ok(reporter.finish())
  }

  pub fn log_path(&self) -> std::path::PathBuf {
    self.project.join("build").join("wkit-build.log")
  }

  fn inferred_test_option(&self, path: &str) -> Result<Option<String>> {
    let recipe = self.query(
      &["inspect", path],
      &[],
      &format!("could not inspect `{path}/conanfile.py`"),
    )?;
    let defined = &recipe["options_definitions"];
    Ok(
      ConanBuild::TEST_OPTIONS
        .into_iter()
        .find(|option| defined.get(option).is_some())
        .map(ToOwned::to_owned),
    )
  }

  fn workspace_packages(&self) -> Result<Vec<(String, Option<PackageRef>)>> {
    let info = self.query(&["workspace", "info"], &[], "could not read the workspace")?;
    Ok(
      info["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|package| {
          let path = package["path"].as_str()?;
          let reference = package["ref"].as_str().map(PackageRef::parse);
          Some((path.to_owned(), reference))
        })
        .collect(),
    )
  }

  fn profile(&self) -> Option<&str> {
    self
      .options
      .profile
      .as_deref()
      .or(self.config().profile_host.as_deref())
  }

  fn profile_args(&self) -> Vec<String> {
    let config = self.config();
    let mut args = Vec::new();
    if let Some(profile) = self.profile() {
      args.extend(["-pr:h".to_owned(), profile.to_owned()]);
    }
    if let Some(profile) = &config.profile_build {
      args.extend(["-pr:b".to_owned(), profile.clone()]);
    }
    args.extend(config.setting_args());
    args
  }

  fn config(&self) -> &'a ConanBuild {
    &self.project.manifest.build.conan
  }

  fn conan(&self) -> ProcessBuilder {
    ProcessBuilder::new("conan").cwd(&self.project.root)
  }

  fn query(&self, command: &[&str], args: &[String], what: &str) -> Result<serde_json::Value> {
    let output = self
      .conan()
      .args(command)
      .args(args)
      .args(["--format", "json"])
      .output_unchecked()?;
    if !output.status.success() {
      let stderr = String::from_utf8_lossy(&output.stderr);
      let reason: Vec<_> = stderr
        .lines()
        .filter_map(|l| l.strip_prefix("ERROR: "))
        .collect();
      return Err(Error::Message(format!("{what}: {}", reason.join("; "))));
    }
    serde_json::from_slice(&output.stdout)
      .map_err(|e| Error::Message(format!("{what}: invalid JSON from conan: {e}")))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn project(toml: &str) -> (tempfile::TempDir, Project) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("wkit.toml"), toml).unwrap();
    std::fs::write(dir.path().join("conanfile.py"), "").unwrap();
    let project = Project::load(&dir.path().join("wkit.toml")).unwrap();
    (dir, project)
  }

  #[test]
  fn command_carries_profiles_settings_and_overrides() {
    let (_dir, project) = project(
      "[project]\nlanguages = [\"cpp\"]\n\n[build.conan]\nprofile-build = \"gcc\"\nsettings = { \"compiler.cppstd\" = \"20\" }\nconf = { \"tools.cmake.cmaketoolchain:generator\" = \"Ninja\" }\nargs = [\"-vwarning\"]\n",
    );
    let options = BuildOptions {
      build_type: Some(BuildType::Release),
      profile: Some("clang".into()),
      jobs: Some(4),
      offline: true,
      extra: vec!["--lockfile-partial".into()],
      ..BuildOptions::default()
    };
    let conan = Conan::new(&project, &options);
    assert!(!conan.is_workspace());
    assert_eq!(conan.build_type().unwrap(), BuildType::Release);
    let test = ["-o".to_owned(), "&:test=True".to_owned()];
    assert_eq!(
      conan.command(BuildType::Release, &test).to_string(),
      "conan build . -pr:h clang -pr:b gcc -s compiler.cppstd=20 -s build_type=Release -o '&:test=True' -c tools.cmake.cmaketoolchain:generator=Ninja -c tools.build:jobs=4 --build=missing --no-remote -vwarning --lockfile-partial"
    );
  }

  #[test]
  fn profile_build_type_is_kept_unless_overridden() {
    let (_dir, project) =
      project("[project]\nlanguages = [\"cpp\"]\n\n[build.conan]\nworkspace = true\n");
    let options = BuildOptions::default();
    let conan = Conan::new(&project, &options);
    assert!(conan.is_workspace());
    assert_eq!(
      conan.command(BuildType::Debug, &[]).to_string(),
      "conan workspace build --build=missing"
    );
  }
}
