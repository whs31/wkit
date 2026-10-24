use wkit_build::Cargo;
use wkit_core::{ProcessBuilder, Shell, version::Flavor};

use crate::{DebControl, Error, Packaging, Result, ToolRun};

#[derive(Debug, Clone, Copy)]
pub struct CargoDeb<'p> {
  packaging: &'p Packaging<'p>,
}

impl<'p> CargoDeb<'p> {
  pub fn new(packaging: &'p Packaging<'p>) -> Self {
    Self { packaging }
  }

  pub fn command(&self, output: &std::path::Path, extra: &[String]) -> ProcessBuilder {
    let project = self.packaging.project();
    let config = &project.manifest.build.cargo;
    let mut cargo = config.command(&project.root, &["deb", "--no-build"]);
    if let Some(package) = &config.package {
      cargo = cargo.args(["--package", package]);
    }
    cargo.arg("--output").arg(output).args(extra)
  }

  pub fn output_path(&self) -> Result<std::path::PathBuf> {
    Ok(self.packaging.output().join(format!(
      "{}_{}_{}.deb",
      self.packaging.name(),
      self.packaging.version()?.render(Flavor::Debian),
      DebControl::host_architecture()
    )))
  }

  pub fn run(&self, shell: &mut Shell, extra: &[String]) -> Result<std::path::PathBuf> {
    let packaging = self.packaging;
    if which::which("cargo-deb").is_err() {
      return Err(Error::setup(
        "Debian packages of cargo projects need cargo-deb",
        Some("install it with `cargo install cargo-deb`".to_owned()),
      ));
    }
    let output = self.output_path()?;
    Cargo::new(packaging.project(), packaging.options()).run(packaging.build_type(), false)?;
    shell.status(
      "Packaging",
      format_args!("{} v{} (cargo-deb)", packaging.name(), packaging.version()?),
    )?;
    std::fs::create_dir_all(packaging.output())?;
    ToolRun::new(shell, packaging.log_path())?.run(
      &self.command(&output, extra),
      "Packaging",
      |_| None,
    )?;
    shell.status("Packaged", packaging.shown(&output))?;
    Ok(output)
  }
}
