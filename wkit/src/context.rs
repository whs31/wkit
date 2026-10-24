use wkit_core::Shell;
use wkit_manifest::{ManifestError, Project};

#[derive(Debug)]
pub struct Context<'a> {
  pub shell: &'a mut Shell,
  pub cwd: std::path::PathBuf,
  pub manifest_path: Option<std::path::PathBuf>,
}

impl Context<'_> {
  pub fn version() -> semver::Version {
    semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("package version is semver")
  }

  pub fn project(&mut self) -> anyhow::Result<Project> {
    let project = self.load()?;
    project.ensure_compatible(&Self::version())?;
    Ok(project)
  }

  pub fn try_project(&mut self) -> anyhow::Result<Option<Project>> {
    match self.load() {
      Ok(project) => Ok(Some(project)),
      Err(ManifestError::NotFound { .. }) => Ok(None),
      Err(e) => Err(e.into()),
    }
  }

  fn load(&mut self) -> Result<Project, ManifestError> {
    let project = match &self.manifest_path {
      Some(path) => Project::load(&self.cwd.join(path))?,
      None => Project::discover(&self.cwd)?,
    };
    for warning in &project.warnings {
      let _ = self.shell.warn(warning);
    }
    Ok(project)
  }
}
