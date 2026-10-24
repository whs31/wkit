use wkit_core::ProcessBuilder;
use wkit_manifest::{BuildType, Project};

use crate::{BuildOptions, Result};

#[derive(Debug, Clone, Copy)]
pub struct Cargo<'a> {
  project: &'a Project,
  options: &'a BuildOptions,
}

impl<'a> Cargo<'a> {
  pub fn new(project: &'a Project, options: &'a BuildOptions) -> Self {
    Self { project, options }
  }

  pub fn applies(project: &Project) -> bool {
    project.manifest.has(wkit_core::Language::Rust) && project.join("Cargo.toml").is_file()
  }

  pub fn command(&self, build_type: BuildType, extra: bool) -> ProcessBuilder {
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

  pub fn run(&self, build_type: BuildType, extra: bool) -> Result<()> {
    self.command(build_type, extra).status().map_err(Into::into)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn optimized_build_types_build_in_release() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
      dir.path().join("wkit.toml"),
      "[project]\nlanguages = [\"rust\"]\n\n[build.cargo]\npackage = \"cli\"\nlocked = true\n",
    )
    .unwrap();
    let project = Project::load(&dir.path().join("wkit.toml")).unwrap();
    let options = BuildOptions {
      offline: true,
      extra: vec!["--timings".into()],
      ..BuildOptions::default()
    };
    let cargo = Cargo::new(&project, &options);
    assert_eq!(
      cargo.command(BuildType::RelWithDebInfo, true).to_string(),
      "cargo build --locked --package cli --release --offline --timings"
    );
    assert_eq!(
      cargo.command(BuildType::Debug, false).to_string(),
      "cargo build --locked --package cli --offline"
    );
  }
}
