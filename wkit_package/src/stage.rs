use wkit_core::{ProcessBuilder, Shell};

use crate::{FileSource, Packaging, Result, ToolRun};

#[derive(Debug)]
pub struct Stage<'p> {
  packaging: &'p Packaging<'p>,
  format: &'static str,
  dir: std::path::PathBuf,
}

impl<'p> Stage<'p> {
  pub fn prepare(packaging: &'p Packaging<'p>, format: &'static str) -> Result<Self> {
    let dir = packaging.stage_dir(format);
    if dir.exists() {
      std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    Ok(Self {
      packaging,
      format,
      dir,
    })
  }

  pub fn dir(&self) -> &std::path::Path {
    &self.dir
  }

  pub fn path(&self, absolute: &std::path::Path) -> std::path::PathBuf {
    self
      .dir
      .join(absolute.strip_prefix("/").unwrap_or(absolute))
  }

  pub fn install_tree(
    &self,
    shell: &mut Shell,
    build_dir: &std::path::Path,
    prefix: &std::path::Path,
  ) -> Result<()> {
    let target = self.path(prefix);
    let install = ProcessBuilder::new("cmake")
      .cwd(&self.packaging.project().root)
      .arg("--install")
      .arg(build_dir)
      .arg("--prefix")
      .arg(&target)
      .args(["--config", &self.packaging.build_type().to_string()]);
    ToolRun::new(shell, self.packaging.log_path())?.run(&install, "Installing", |_| None)?;
    for excluded in &self.packaging.project().manifest.package.exclude {
      let path = target.join(excluded);
      if path.is_dir() {
        std::fs::remove_dir_all(&path)?;
      } else if path.exists() {
        std::fs::remove_file(&path)?;
      }
    }
    Ok(())
  }

  pub fn add_files(&self) -> Result<usize> {
    let project = self.packaging.project();
    let mut count = 0;
    for mapping in &project.manifest.package.files {
      for file in FileSource::expand(&project.root, mapping, self.packaging.build_type())? {
        let target = self.path(&file.destination);
        Self::copy(&file.source, &target, file.mode.0)?;
        count += 1;
      }
    }
    Ok(count)
  }

  pub fn add_license(&self, destination: &std::path::Path) -> Result<bool> {
    let project = self.packaging.project();
    let license = match &project.manifest.package.license {
      Some(license) => Some(project.join(license)),
      None => Some(project.join("LICENSE")).filter(|path| path.is_file()),
    };
    let Some(license) = license else {
      return Ok(false);
    };
    Self::copy(&license, &self.path(destination), 0o644)?;
    Ok(true)
  }

  pub fn run_hook(&self, shell: &mut Shell, build_dir: Option<&std::path::Path>) -> Result<()> {
    let project = self.packaging.project();
    let Some(hook) = &project.manifest.package.stage else {
      return Ok(());
    };
    let mut process = self
      .packaging
      .with_env(hook.to_process(&project.root))
      .env("WKIT_STAGE_DIR", &self.dir)
      .env("WKIT_PACKAGE_FORMAT", self.format);
    if let Some(build_dir) = build_dir {
      process = process.env("WKIT_BUILD_DIR", build_dir);
    }
    shell.status(
      "Staging",
      format_args!("running `{}`", hook.run.line_with(&[])),
    )?;
    process.status()?;
    Ok(())
  }

  fn copy(source: &std::path::Path, target: &std::path::Path, mode: u32) -> Result<()> {
    if let Some(parent) = target.parent() {
      std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(source, target).map_err(|e| wkit_core::Error::Write {
      path: target.to_owned(),
      source: e,
    })?;
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      std::fs::set_permissions(target, std::fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(())
  }
}
