use std::path::{Path, PathBuf};

use anyhow::Context as _;

use super::{BuildOptions, BuiltPackage, PackageRef, log::Reporter};
use crate::{
  ProcessBuilder, Result, Shell,
  manifest::{BuildType, ConanBuild, ConanSync, Project},
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

  fn is_workspace(&self) -> bool {
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
    let value = profile["host"]["settings"]["build_type"].as_str().with_context(|| {
      format!("the `{profile_name}` profile does not set `build_type`; set it there or pass `--build-type`")
    })?;
    value.parse().map_err(anyhow::Error::msg)
  }

  fn command(
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

  fn graph_command(&self, build_type: BuildType, format: &str, output: &Path) -> ProcessBuilder {
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

  fn test_options(&self) -> Result<Vec<String>> {
    if !self.options.tests {
      return Ok(Vec::new());
    }
    let mut args = Vec::new();
    for package in self.packages()? {
      let option = match &self.config().test_option {
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

  fn reporter<'s>(
    &self,
    shell: &'s mut Shell,
    build_type: BuildType,
    log: bool,
  ) -> Result<Reporter<'s>> {
    let log = log.then(|| self.project.join("build").join("wkit-build.log"));
    Reporter::new(shell, PackageRef::of_project(self.project), build_type, log)
  }

  fn execute(
    &self,
    shell: &mut Shell,
    action: &str,
    build_type: BuildType,
    tests: bool,
  ) -> Result<Vec<BuiltPackage>> {
    let test_options = if tests {
      self.test_options()?
    } else {
      Vec::new()
    };
    let mut reporter = self.reporter(shell, build_type, true)?;
    reporter.run(&self.command(action, build_type, &test_options))?;
    Ok(reporter.finish())
  }

  pub fn run(&self, shell: &mut Shell, build_type: BuildType) -> Result<Vec<BuiltPackage>> {
    self.execute(shell, "build", build_type, true)
  }

  pub fn create(&self, shell: &mut Shell, build_type: BuildType) -> Result<()> {
    self.execute(shell, "create", build_type, false).map(drop)
  }

  pub fn configure(&self, shell: &mut Shell, build_type: BuildType) -> Result<Vec<BuiltPackage>> {
    let command = self.command("install", build_type, &self.test_options()?);
    let mut reporter = self.reporter(shell, build_type, true)?;
    reporter.run(&command)?;
    let packages = self.packages()?;
    let last = packages.len().saturating_sub(1);
    for (index, package) in packages.into_iter().enumerate() {
      let dir = self.project.join(&package.path);
      let preset = configure_preset(&dir, build_type).with_context(|| {
        format!(
          "Conan generated no CMake preset for `{build_type}` in `{}`; does the conanfile use `CMakeToolchain` and `cmake_layout`?",
          dir.display()
        )
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

  pub fn graph(&self, shell: &mut Shell, build_type: BuildType, output: &Path) -> Result<()> {
    let mut reporter = self.reporter(shell, build_type, false)?;
    reporter.run(&self.graph_command(build_type, "html", output))?;
    reporter.finish();
    Ok(())
  }

  pub fn default_option(&self, name: &str) -> Result<Option<String>> {
    let recipe = self.query(&["inspect", "."], &[], "could not inspect `conanfile.py`")?;
    Ok(recipe["options"][name].as_str().map(ToOwned::to_owned))
  }

  fn query(&self, command: &[&str], args: &[String], what: &str) -> Result<serde_json::Value> {
    let output = self
      .conan()
      .args(command)
      .args(args)
      .args(["--format", "json"])
      .output_unchecked()?;
    anyhow::ensure!(
      output.status.success(),
      "{what}: {}",
      errors(&output.stderr)
    );
    serde_json::from_slice(&output.stdout)
      .with_context(|| format!("{what}: invalid JSON from conan"))
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

  fn option_args(&self) -> Vec<String> {
    self
      .options
      .conan_options
      .iter()
      .flat_map(|option| ["-o".to_owned(), format!("&:{option}")])
      .collect()
  }

  fn packages(&self) -> Result<Vec<WorkspacePackage>> {
    let single = || {
      vec![WorkspacePackage {
        path: ".".into(),
        reference: None,
      }]
    };
    if !self.is_workspace() {
      return Ok(single());
    }
    Ok(ConanWorkspace::load(&self.project.root)?.map_or_else(single, |w| w.packages))
  }

  fn inferred_test_option(&self, path: &Path) -> Result<Option<String>> {
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

fn errors(stderr: &[u8]) -> String {
  let stderr = String::from_utf8_lossy(stderr);
  stderr
    .lines()
    .filter_map(|l| l.strip_prefix("ERROR: "))
    .collect::<Vec<_>>()
    .join("; ")
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
  serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn configure_preset(dir: &Path, build_type: BuildType) -> Option<String> {
  let user = read_json(&dir.join("CMakeUserPresets.json"))?;
  let mut fallback = None;
  for include in user["include"].as_array()?.iter().rev() {
    let Some(presets) = include.as_str().and_then(|i| read_json(&dir.join(i))) else {
      continue;
    };
    for preset in presets["configurePresets"].as_array().into_iter().flatten() {
      let name = preset["name"].as_str().map(ToOwned::to_owned);
      match preset["cacheVariables"]["CMAKE_BUILD_TYPE"].as_str() {
        Some(t) if t == build_type.to_string() => return name,
        Some(_) => {}
        None => fallback = fallback.or(name),
      }
    }
  }
  fallback
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePackage {
  pub path: PathBuf,
  pub reference: Option<PackageRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConanWorkspace {
  pub packages: Vec<WorkspacePackage>,
}

impl ConanWorkspace {
  const YAML: &str = "conanws.yml";
  const PYTHON: &str = "conanws.py";

  pub fn exists(root: &Path) -> bool {
    root.join(Self::YAML).is_file() || root.join(Self::PYTHON).is_file()
  }

  pub fn load(root: &Path) -> Result<Option<Self>> {
    let python = root.join(Self::PYTHON).is_file();
    if let Some(text) = crate::fs::read_optional(&root.join(Self::YAML))? {
      let workspace = Self::parse(&text);
      if !workspace.packages.is_empty() || !python {
        return Ok(Some(workspace));
      }
    }
    python.then(|| Self::query(root)).transpose()
  }

  fn parse(text: &str) -> Self {
    let mut packages: Vec<WorkspacePackage> = Vec::new();
    let mut in_packages = false;
    for line in text.lines() {
      let trimmed = line.trim();
      if trimmed.is_empty() || trimmed.starts_with('#') {
        continue;
      }
      if !line.starts_with([' ', '\t', '-']) {
        in_packages = trimmed == "packages:";
        continue;
      }
      if !in_packages {
        continue;
      }
      let field = match trimmed.strip_prefix("- ") {
        Some(field) => {
          packages.push(WorkspacePackage {
            path: PathBuf::new(),
            reference: None,
          });
          field
        }
        None => trimmed,
      };
      let (Some(package), Some((key, value))) = (packages.last_mut(), field.split_once(':')) else {
        continue;
      };
      let value = value.trim().trim_matches(['"', '\'']);
      match key.trim() {
        "path" => package.path = value.into(),
        "ref" => package.reference = Some(PackageRef::parse(value)),
        _ => {}
      }
    }
    packages.retain(|p| !p.path.as_os_str().is_empty());
    Self { packages }
  }

  fn query(root: &Path) -> Result<Self> {
    let output = ProcessBuilder::new("conan")
      .cwd(root)
      .args(["workspace", "info", "--format", "json"])
      .output()?;
    let info: serde_json::Value =
      serde_json::from_slice(&output.stdout).context("could not read the workspace")?;
    let packages = info["packages"]
      .as_array()
      .into_iter()
      .flatten()
      .filter_map(|package| {
        Some(WorkspacePackage {
          path: package["path"].as_str()?.into(),
          reference: package["ref"].as_str().map(PackageRef::parse),
        })
      })
      .collect();
    Ok(Self { packages })
  }
}

#[derive(Debug, Clone)]
pub struct RemoteSync<'a> {
  project: &'a Project,
  options: BuildOptions,
  config: ConanSync,
  dry_run: bool,
}

impl<'a> RemoteSync<'a> {
  pub fn new(
    project: &'a Project,
    options: &BuildOptions,
    remote: Option<String>,
    dry_run: bool,
  ) -> Self {
    let mut config = project.manifest.conan.sync.clone();
    config.remote = remote.or(config.remote);
    Self {
      project,
      options: BuildOptions {
        offline: true,
        ..options.clone()
      },
      config,
      dry_run,
    }
  }

  pub fn remote(&self) -> Option<&str> {
    self.config.remote.as_deref()
  }

  pub fn run(&self, shell: &mut Shell, build_type: BuildType) -> Result<usize> {
    let remote = self
      .remote()
      .context("no remote to sync with: set `conan.sync.remote` or pass `--remote`")?;
    let conan = Conan::new(self.project, &self.options);
    let remotes = conan.query(&["remote", "list"], &[], "could not list the Conan remotes")?;
    let found = remotes
      .as_array()
      .into_iter()
      .flatten()
      .find(|r| r["name"].as_str() == Some(remote))
      .with_context(|| format!("Conan remote `{remote}` is not configured"))?;
    anyhow::ensure!(
      found["enabled"].as_bool() != Some(false),
      "Conan remote `{remote}` is disabled"
    );

    let work = tempfile::tempdir()?;
    let graph_path = work.path().join("graph.json");
    let bar = shell.transient("Resolving");
    bar.set_message("dependencies from the local cache");
    let graph = checked(
      &conan.graph_command(build_type, "json", &graph_path),
      "resolving the dependency graph",
    );
    bar.finish_and_clear();
    graph?;
    let graph = read(&graph_path)?;
    ensure_cached(&graph)?;

    let list_path = work.path().join("packages.json");
    checked(
      &conan
        .conan()
        .args(["list", "--graph"])
        .arg(&graph_path)
        .args(["--format", "json", "--out-file"])
        .arg(&list_path),
      "listing the cached packages",
    )?;
    let Some(serde_json::Value::Object(mut cache)) = read(&list_path)?
      .get_mut("Local Cache")
      .map(serde_json::Value::take)
    else {
      anyhow::bail!("`conan list` returned no local cache section");
    };
    if let Some(root) = root_reference(&graph) {
      cache.remove(&root);
    }

    let action = if self.dry_run {
      "Checking"
    } else {
      "Uploading"
    };
    let bar = shell.progress(action, cache.len() as u64);
    for (index, (reference, details)) in cache.iter().enumerate() {
      let package = PackageRef::parse(reference);
      bar.set_message(format!(": {package}"));
      shell.status(action, &package)?;
      let list = work.path().join(format!("upload-{index}.json"));
      fs_err::write(
        &list,
        serde_json::json!({ "Local Cache": { reference: details } }).to_string(),
      )?;
      let mut upload =
        conan
          .conan()
          .args(["upload", "--list"])
          .arg(&list)
          .args(["--remote", remote, "--confirm"]);
      for (flag, enabled) in [
        ("--check", self.config.check),
        ("--force", self.config.force),
        ("--dry-run", self.dry_run),
      ] {
        if enabled {
          upload = upload.arg(flag);
        }
      }
      checked(&upload, &format!("uploading `{package}`"))?;
      bar.inc(1);
    }
    bar.finish_and_clear();
    Ok(cache.len())
  }
}

fn ensure_cached(graph: &serde_json::Value) -> Result<()> {
  let mut missing: Vec<String> = graph["graph"]["nodes"]
    .as_object()
    .into_iter()
    .flat_map(serde_json::Map::values)
    .filter(|node| matches!(node["binary"].as_str(), Some("Missing" | "Invalid")))
    .map(|node| {
      let reference = PackageRef::parse(node["ref"].as_str().unwrap_or("unknown"));
      match node["package_id"].as_str() {
        Some(id) if !id.is_empty() => format!("{reference} ({id})"),
        _ => reference.to_string(),
      }
    })
    .collect();
  missing.sort();
  missing.dedup();
  anyhow::ensure!(
    missing.is_empty(),
    "binaries missing from the local cache; run `wkit build` first:\n  {}",
    missing.join("\n  ")
  );
  Ok(())
}

fn root_reference(graph: &serde_json::Value) -> Option<String> {
  let root = graph["graph"]["root"].as_object()?.keys().next()?;
  let reference = graph["graph"]["nodes"][root]["ref"]
    .as_str()?
    .split('#')
    .next()?;
  (!reference.is_empty() && reference != "conanfile").then(|| reference.to_owned())
}

fn read(path: &Path) -> Result<serde_json::Value> {
  serde_json::from_str(&fs_err::read_to_string(path)?)
    .with_context(|| format!("could not parse `{}`", path.display()))
}

fn checked(process: &ProcessBuilder, what: &str) -> Result<()> {
  let output = process.output_unchecked()?;
  anyhow::ensure!(
    output.status.success(),
    "{what} failed: {}",
    errors(&output.stderr)
  );
  Ok(())
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
        .graph_command(BuildType::Debug, "html", Path::new("docs/graph.html"))
        .to_string(),
      "conan graph info . --format html --out-file docs/graph.html"
    );
  }

  #[test]
  fn picks_the_preset_for_the_build_type() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for (folder, name, build_type) in [
      ("build/Debug", "conan-debug", Some("Debug")),
      ("build/Release", "conan-release", Some("Release")),
      ("build/msvc", "conan-default", None),
    ] {
      let generators = root.join(folder).join("generators");
      std::fs::create_dir_all(&generators).unwrap();
      let cache = build_type
        .map(|t| format!(r#", "cacheVariables": {{"CMAKE_BUILD_TYPE": "{t}"}}"#))
        .unwrap_or_default();
      std::fs::write(
        generators.join("CMakePresets.json"),
        format!(r#"{{"configurePresets": [{{"name": "{name}"{cache}}}]}}"#),
      )
      .unwrap();
    }
    let user = root.join("CMakeUserPresets.json");
    std::fs::write(
      &user,
      r#"{"include": ["build/Debug/generators/CMakePresets.json", "build/Release/generators/CMakePresets.json"]}"#,
    )
    .unwrap();
    assert_eq!(
      configure_preset(root, BuildType::Release).as_deref(),
      Some("conan-release")
    );
    assert_eq!(configure_preset(root, BuildType::MinSizeRel), None);
    std::fs::write(
      &user,
      r#"{"include": ["build/msvc/generators/CMakePresets.json"]}"#,
    )
    .unwrap();
    assert_eq!(
      configure_preset(root, BuildType::MinSizeRel).as_deref(),
      Some("conan-default")
    );
  }

  #[test]
  fn reads_packages_from_conanws_yml() {
    let workspace = ConanWorkspace::parse(
      "# radar\npackages:\n  - path: core\n    ref: radar-core/0.5.0\n  - ref: \"radar-app/0.5.0\"\n    path: 'app'\n  - path: tools\nother:\n  - path: ignored\n",
    );
    let found: Vec<_> = workspace
      .packages
      .iter()
      .map(|p| {
        format!(
          "{} {}",
          p.path.display(),
          p.reference
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default()
        )
      })
      .collect();
    assert_eq!(
      found,
      ["core radar-core v0.5.0", "app radar-app v0.5.0", "tools "]
    );
  }

  #[test]
  fn graph_checks_find_the_root_and_missing_binaries() {
    let graph = serde_json::json!({
      "graph": {
        "root": { "0": "conanfile.py (geodesy/2.4.1)" },
        "nodes": {
          "0": { "ref": "geodesy/2.4.1", "binary": null },
          "1": { "ref": "fmt/12.1.0#abc", "binary": "Cache", "package_id": "44" },
          "2": { "ref": "zlib/1.3.1#def", "binary": "Missing", "package_id": "fc02" }
        }
      }
    });
    assert_eq!(root_reference(&graph).as_deref(), Some("geodesy/2.4.1"));
    let err = ensure_cached(&graph).unwrap_err().to_string();
    assert!(err.ends_with("\n  zlib v1.3.1 (fc02)"), "{err}");
  }
}
