use wkit_core::{ProcessBuilder, Shell};
use wkit_manifest::{BuildType, Project, build::ConanBuild};

use crate::{
  BuildOptions, BuiltPackage, ConanPresets, ConanWorkspace, Error, PackageRef, Reporter, Result,
  WorkspacePackage,
};

#[derive(Debug, Clone, Copy)]
pub struct Conan<'a> {
  project: &'a Project,
  options: &'a BuildOptions,
}

impl<'a> Conan<'a> {
  pub fn new(project: &'a Project, options: &'a BuildOptions) -> Self {
    Self { project, options }
  }

  pub fn is_workspace(&self) -> bool {
    self.config().workspace
      || (!self.project.join("conanfile.py").is_file()
        && ConanWorkspace::exists(&self.project.root))
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

  pub fn command(
    &self,
    action: &str,
    build_type: BuildType,
    test_options: &[String],
  ) -> ProcessBuilder {
    let config = self.config();
    let mut conan = self.conan();
    conan = if self.is_workspace() {
      conan.args(["workspace", action])
    } else {
      conan.args([action, "."])
    };
    conan = conan.args(self.profile_args());
    if self.options.build_type.is_some() {
      conan = conan.arg("-s").arg(format!("build_type={build_type}"));
    }
    conan = conan
      .args(test_options)
      .args(self.option_args())
      .args(config.conf_args());
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

  pub fn graph_command(
    &self,
    build_type: BuildType,
    format: &str,
    output: &std::path::Path,
  ) -> ProcessBuilder {
    let mut conan = self
      .conan()
      .args(["graph", "info", "."])
      .args(self.profile_args());
    if self.options.build_type.is_some() {
      conan = conan.arg("-s").arg(format!("build_type={build_type}"));
    }
    if self.options.offline {
      conan = conan.arg("--no-remote");
    }
    conan
      .args(self.option_args())
      .args(self.config().conf_args())
      .args(["--format", format, "--out-file"])
      .arg(output)
  }

  pub fn test_options(&self) -> Result<Vec<String>> {
    if !self.options.tests {
      return Ok(Vec::new());
    }
    let config = self.config();
    let mut args = Vec::new();
    for package in self.packages()? {
      let option = match &config.test_option {
        Some(option) => Some(option.clone()),
        None => self.inferred_test_option(&package.path)?,
      };
      if let Some(option) = option {
        let scope = package
          .reference
          .map_or_else(|| "&".to_owned(), |r| format!("{}/*", r.name));
        args.extend(["-o".to_owned(), format!("{scope}:{option}=True")]);
      }
    }
    Ok(args)
  }

  pub fn run(&self, shell: &mut Shell, build_type: BuildType) -> Result<Vec<BuiltPackage>> {
    let command = self.command("build", build_type, &self.test_options()?);
    let mut reporter = self.reporter(shell, build_type)?;
    reporter.run(&command)?;
    Ok(reporter.finish())
  }

  pub fn configure(&self, shell: &mut Shell, build_type: BuildType) -> Result<Vec<BuiltPackage>> {
    let command = self.command("install", build_type, &self.test_options()?);
    let mut reporter = self.reporter(shell, build_type)?;
    reporter.run(&command)?;
    let packages = self.packages()?;
    let last = packages.len().saturating_sub(1);
    for (index, package) in packages.into_iter().enumerate() {
      let dir = self.project.join(&package.path);
      let preset = ConanPresets::configure_preset(&dir, build_type).ok_or_else(|| {
        Error::Message(format!(
          "Conan generated no CMake preset for `{build_type}` in `{}`; does the conanfile use `CMakeToolchain` and `cmake_layout`?",
          dir.display()
        ))
      })?;
      reporter.set_root(
        package
          .reference
          .unwrap_or_else(|| PackageRef::of_project(self.project)),
      );
      let cmake = ProcessBuilder::new("cmake").cwd(&dir);
      reporter.run(&cmake.clone().args(["--preset", &preset]))?;
      if index < last {
        reporter.start_build();
        reporter.run(&cmake.args(["--build", "--preset", &preset]))?;
      }
    }
    Ok(reporter.finish())
  }

  pub fn create(&self, shell: &mut Shell, build_type: BuildType) -> Result<()> {
    let command = self.command("create", build_type, &[]);
    let mut reporter = self.reporter(shell, build_type)?;
    reporter.run(&command)?;
    reporter.finish();
    Ok(())
  }

  pub fn graph(
    &self,
    shell: &mut Shell,
    build_type: BuildType,
    output: &std::path::Path,
  ) -> Result<()> {
    let mut reporter = Reporter::new(
      shell,
      PackageRef::of_project(self.project),
      build_type,
      None,
    )?;
    reporter.run(&self.graph_command(build_type, "html", output))?;
    reporter.finish();
    Ok(())
  }

  pub fn default_option(&self, name: &str) -> Result<Option<String>> {
    let recipe = self.query(&["inspect", "."], &[], "could not inspect `conanfile.py`")?;
    Ok(recipe["options"][name].as_str().map(ToOwned::to_owned))
  }

  pub fn log_path(&self) -> std::path::PathBuf {
    self.project.join("build").join("wkit-build.log")
  }

  pub fn query(&self, command: &[&str], args: &[String], what: &str) -> Result<serde_json::Value> {
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

  pub fn profile_args(&self) -> Vec<String> {
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

  fn option_args(&self) -> Vec<String> {
    self
      .options
      .conan_options
      .iter()
      .flat_map(|option| ["-o".to_owned(), format!("&:{option}")])
      .collect()
  }

  fn reporter<'s>(&self, shell: &'s mut Shell, build_type: BuildType) -> Result<Reporter<'s>> {
    Reporter::new(
      shell,
      PackageRef::of_project(self.project),
      build_type,
      Some(self.log_path()),
    )
  }

  fn packages(&self) -> Result<Vec<WorkspacePackage>> {
    let single = || WorkspacePackage {
      path: ".".into(),
      reference: None,
    };
    if !self.is_workspace() {
      return Ok(vec![single()]);
    }
    Ok(ConanWorkspace::load(&self.project.root)?.map_or_else(|| vec![single()], |w| w.packages))
  }

  fn inferred_test_option(&self, path: &std::path::Path) -> Result<Option<String>> {
    let path = path.to_string_lossy();
    let recipe = self.query(
      &["inspect", &path],
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

  fn profile(&self) -> Option<&str> {
    self
      .options
      .profile
      .as_deref()
      .or(self.config().profile_host.as_deref())
  }

  fn config(&self) -> &'a ConanBuild {
    &self.project.manifest.build.conan
  }

  fn conan(&self) -> ProcessBuilder {
    ProcessBuilder::new("conan").cwd(&self.project.root)
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
      conan_options: vec!["deployment_set=field".into()],
      ..BuildOptions::default()
    };
    let conan = Conan::new(&project, &options);
    assert!(!conan.is_workspace());
    assert_eq!(conan.build_type().unwrap(), BuildType::Release);
    let test = ["-o".to_owned(), "&:test=True".to_owned()];
    assert_eq!(
      conan
        .command("build", BuildType::Release, &test)
        .to_string(),
      "conan build . -pr:h clang -pr:b gcc -s compiler.cppstd=20 -s build_type=Release -o '&:test=True' -o '&:deployment_set=field' -c tools.cmake.cmaketoolchain:generator=Ninja -c tools.build:jobs=4 --build=missing --no-remote -vwarning --lockfile-partial"
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
      conan.command("build", BuildType::Debug, &[]).to_string(),
      "conan workspace build --build=missing"
    );
    assert_eq!(
      conan
        .graph_command(
          BuildType::Debug,
          "html",
          std::path::Path::new("docs/graph.html")
        )
        .to_string(),
      "conan graph info . --format html --out-file docs/graph.html"
    );
  }
}
