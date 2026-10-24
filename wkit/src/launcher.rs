use wkit_build::{BuildOptions, Builder, Cargo};
use wkit_core::{ProcessBuilder, Shell};
use wkit_manifest::{Bin, BuildType, Project};

#[derive(Debug)]
pub struct Launcher<'p> {
  project: &'p Project,
  options: &'p BuildOptions,
  bin: Bin,
  named: bool,
}

impl<'p> Launcher<'p> {
  pub fn new(
    project: &'p Project,
    options: &'p BuildOptions,
    name: Option<&str>,
  ) -> anyhow::Result<Self> {
    let manifest = &project.manifest;
    let bin = match name {
      Some(name) => manifest
        .bin(name)
        .cloned()
        .unwrap_or_else(|| Self::bare(name)),
      None => match manifest.default_bin() {
        Some(bin) => bin.clone(),
        None if !manifest.bins.is_empty() => anyhow::bail!(
          "several `[[bin]]` entries and no `project.default-run`; pass a bin name, one of: {}",
          manifest
            .bins
            .iter()
            .map(|b| b.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
        ),
        None => Self::bare(&project.name()),
      },
    };
    let named = name.is_some() || manifest.bin(&bin.name).is_some();
    Ok(Self {
      project,
      options,
      bin,
      named,
    })
  }

  pub fn build(&self, shell: &mut Shell, through_cargo: bool) -> anyhow::Result<ProcessBuilder> {
    let builder = Builder::new(self.project, self.options);
    let native = self.bin.path.is_some() || (self.bin.package.is_none() && builder.builds_native());
    let process = if native {
      let outcome = builder.run_native(shell)?;
      let build_type = match &outcome {
        Some(outcome) => outcome.build_type,
        None => builder.build_type()?,
      };
      ProcessBuilder::new(
        self.executable(build_type, outcome.as_ref().and_then(|o| o.build_dir()))?,
      )
    } else if Cargo::applies(self.project) {
      let build_type = self.options.build_type.unwrap_or(BuildType::Debug);
      let cargo = Cargo::new(self.project, self.options);
      if through_cargo {
        cargo.run_command(
          build_type,
          self.bin.package.as_deref(),
          self.named.then_some(self.bin.name.as_str()),
        )
      } else {
        cargo.run(build_type, false)?;
        ProcessBuilder::new(self.cargo_executable(build_type)?)
      }
    } else {
      anyhow::bail!("nothing to run: the project has no Conan, CMake or Cargo build");
    };
    let mut process = process.args(&self.bin.args);
    if let Some(dir) = &self.bin.cwd {
      process = process.cwd(self.project.join(dir));
    }
    for (key, value) in &self.bin.env {
      process = process.env(key, value);
    }
    Ok(process)
  }

  pub fn display(&self, process: &ProcessBuilder) -> String {
    let program = std::path::Path::new(process.get_program());
    program
      .strip_prefix(&self.project.root)
      .unwrap_or(program)
      .display()
      .to_string()
  }

  fn executable(
    &self,
    build_type: BuildType,
    build_dir: Option<&std::path::Path>,
  ) -> anyhow::Result<std::path::PathBuf> {
    if let Some(path) = self.bin.resolve_path(build_type) {
      let path = self.project.join(path);
      anyhow::ensure!(
        path.is_file(),
        "`{}` does not exist after the build; check `path` in `[[bin]]`",
        path.display()
      );
      return Ok(path);
    }
    let dir = build_dir.ok_or_else(|| anyhow::anyhow!("the build reported no build directory"))?;
    wkit_core::fs::find_executable(dir, &self.bin.name).ok_or_else(|| {
      anyhow::anyhow!(
        "no executable named `{}` in `{}`; add a `[[bin]]` with a `path`",
        self.bin.name,
        dir.display()
      )
    })
  }

  fn cargo_executable(&self, build_type: BuildType) -> anyhow::Result<std::path::PathBuf> {
    let output = ProcessBuilder::new("cargo")
      .cwd(&self.project.root)
      .args(["metadata", "--no-deps", "--format-version", "1"])
      .output()?;
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let target = metadata["target_directory"]
      .as_str()
      .map_or_else(|| self.project.join("target"), std::path::PathBuf::from);
    let profile = if build_type.is_optimized() {
      "release"
    } else {
      "debug"
    };
    let dir = target.join(profile);
    let exe = dir.join(format!("{}{}", self.bin.name, std::env::consts::EXE_SUFFIX));
    anyhow::ensure!(
      exe.is_file(),
      "no binary named `{}` in `{}`; name one of the package's binaries",
      self.bin.name,
      dir.display()
    );
    Ok(exe)
  }

  fn bare(name: &str) -> Bin {
    Bin {
      name: name.to_owned(),
      path: None,
      package: None,
      args: Vec::new(),
      env: std::collections::BTreeMap::new(),
      cwd: None,
    }
  }
}
