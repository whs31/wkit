use wkit_core::Shell;

use crate::{DebArchive, DebControl, Packaging, Result, Stage};

#[derive(Debug, Clone, Copy)]
pub struct DebPackager<'p> {
  packaging: &'p Packaging<'p>,
}

impl<'p> DebPackager<'p> {
  pub fn new(packaging: &'p Packaging<'p>) -> Self {
    Self { packaging }
  }

  pub fn run(&self, shell: &mut Shell) -> Result<std::path::PathBuf> {
    let packaging = self.packaging;
    let project = packaging.project();
    let config = project.manifest.package.deb.clone().unwrap_or_default();
    let mut control = DebControl::new(
      packaging.name(),
      &packaging.version()?,
      &config,
      project.manifest.project.description.as_deref(),
    )?;
    let build_dir = packaging.build_native(shell)?;
    shell.status(
      "Packaging",
      format_args!("{} v{} (deb)", control.name, control.version),
    )?;
    let stage = Stage::prepare(packaging, "deb")?;
    stage.install_tree(shell, &build_dir, &config.prefix)?;
    stage.add_files()?;
    stage.add_license(
      &std::path::Path::new("/usr/share/doc")
        .join(packaging.name())
        .join("copyright"),
    )?;
    stage.run_hook(shell, Some(&build_dir))?;
    control.installed_size = DebArchive::installed_size(stage.dir())?;
    let output = packaging.output().join(control.file_name());
    DebArchive::with_scripts(&project.join(&config.scripts)).write(
      stage.dir(),
      &control.render(),
      &output,
    )?;
    shell.status("Packaged", packaging.shown(&output))?;
    Ok(output)
  }
}
