mod install_method;

use wkit_build::Builder;
use wkit_core::{ProcessBuilder, Shell};

pub use self::install_method::InstallMethod;
use crate::{
  ArchPackager, CargoDeb, DebPackager, Distro, Error, FileSource, Packaging, Privileged, Result,
};

#[derive(Debug, Clone)]
pub struct Installer<'p> {
  packaging: &'p Packaging<'p>,
  privileged: Privileged,
  distro: Distro,
}

impl<'p> Installer<'p> {
  pub const TMPFILES_DIR: &'static str = "/usr/lib/tmpfiles.d";
  pub const UNIT_DIR: &'static str = "/usr/lib/systemd/system";

  pub fn new(packaging: &'p Packaging<'p>, privileged: Privileged, distro: Distro) -> Self {
    Self {
      packaging,
      privileged,
      distro,
    }
  }

  pub fn method(&self, files: bool) -> Result<InstallMethod> {
    let manifest = &self.packaging.project().manifest;
    let has_files = !manifest.install.is_empty();
    if files {
      return if has_files {
        Ok(InstallMethod::Files)
      } else {
        Err(Error::setup(
          "`--files` needs an `[install]` section in wkit.toml",
          None,
        ))
      };
    }
    match self.distro {
      Distro::Debian if manifest.package.deb.is_some() || self.packaging.is_cargo_project() => {
        Ok(InstallMethod::Deb)
      }
      Distro::Arch if ArchPackager::is_available(self.packaging) => Ok(InstallMethod::Arch),
      _ if has_files => Ok(InstallMethod::Files),
      _ => Err(Error::setup(
        format!("nothing to install on {}", self.distro),
        Some("add `[package.deb]`, a PKGBUILD or an `[install]` section to wkit.toml".to_owned()),
      )),
    }
  }

  pub fn install(&self, shell: &mut Shell, files: bool) -> Result<InstallMethod> {
    let method = self.method(files)?;
    let privileged = self.privileged;
    match method {
      InstallMethod::Deb => {
        let deb = if self.packaging.is_cargo_project() {
          CargoDeb::new(self.packaging).run(shell, &[])?
        } else {
          DebPackager::new(self.packaging).run(shell)?
        };
        privileged.run(
          shell,
          &privileged
            .command("apt-get")
            .args(["install", "-y", "--reinstall"])
            .arg(deb),
        )?;
      }
      InstallMethod::Arch => {
        let packages: Vec<_> = ArchPackager::new(self.packaging)?
          .run(shell, privileged, &[])?
          .into_iter()
          .filter(|p| !p.to_string_lossy().contains("-debug-"))
          .collect();
        privileged.run(
          shell,
          &privileged
            .command("pacman")
            .args(["-U", "--noconfirm"])
            .args(&packages),
        )?;
      }
      InstallMethod::Files => self.install_files(shell)?,
    }
    if !privileged.is_dry_run() {
      shell.status(
        "Installed",
        format_args!("{} ({method})", self.packaging.name()),
      )?;
    }
    Ok(method)
  }

  pub fn uninstall(&self, shell: &mut Shell) -> Result<()> {
    let privileged = self.privileged;
    let mut removed = self.remove_packages(shell)?;
    let install = &self.packaging.project().manifest.install;
    let systemctl = which::which("systemctl").is_ok();
    if systemctl {
      for unit in self.units() {
        if unit.exists() {
          let name = unit.file_name().unwrap_or_default();
          privileged.run(
            shell,
            &privileged
              .command("systemctl")
              .args(["disable", "--now"])
              .arg(name),
          )?;
        }
      }
    }
    let paths: Vec<_> = self
      .installed_paths()
      .into_iter()
      .chain(install.remove.iter().cloned())
      .filter(|path| std::fs::symlink_metadata(path).is_ok())
      .collect();
    if !paths.is_empty() {
      privileged.run(shell, &privileged.command("rm").arg("-rf").args(&paths))?;
      if systemctl && !install.services.is_empty() {
        privileged.run(shell, &privileged.command("systemctl").arg("daemon-reload"))?;
      }
      removed = true;
    }
    if !removed {
      shell.warn(format_args!(
        "`{}` is not installed; nothing to remove",
        self.packaging.name()
      ))?;
    } else if !privileged.is_dry_run() {
      shell.status("Removed", self.packaging.name())?;
    }
    Ok(())
  }

  pub fn installed_paths(&self) -> Vec<std::path::PathBuf> {
    let project = self.packaging.project();
    let install = &project.manifest.install;
    let build_type = self.packaging.build_type();
    let mut paths: Vec<std::path::PathBuf> = install.prefix.iter().cloned().collect();
    for mapping in install.files.iter().filter(|m| !m.config) {
      if mapping.is_glob() {
        let found = FileSource::expand(&project.root, mapping, build_type).unwrap_or_default();
        paths.extend(found.into_iter().map(|f| f.destination));
      } else {
        paths.push(mapping.destination(std::path::Path::new(&mapping.source(build_type))));
      }
    }
    paths.extend(install.tmpfiles.iter().map(|t| Self::tmpfiles_path(t)));
    paths.extend(self.units());
    paths
  }

  fn install_files(&self, shell: &mut Shell) -> Result<()> {
    let packaging = self.packaging;
    let project = packaging.project();
    let install = &project.manifest.install;
    let privileged = self.privileged;
    let outcome = Builder::new(project, packaging.options()).run(shell)?;
    if let Some(prefix) = &install.prefix {
      let build_dir = outcome
        .build_dir()
        .ok_or_else(|| Error::setup("`install.prefix` needs a CMake build", None))?;
      if prefix.exists() {
        privileged.run(shell, &privileged.command("rm").arg("-rf").arg(prefix))?;
      }
      privileged.run(
        shell,
        &privileged
          .command("cmake")
          .arg("--install")
          .arg(build_dir)
          .arg("--prefix")
          .arg(prefix)
          .args(["--config", &outcome.build_type.to_string()]),
      )?;
    }
    let install_file = |source: &std::path::Path, destination: &std::path::Path, mode: String| {
      privileged
        .command("install")
        .arg(format!("-Dm{mode}"))
        .arg(source)
        .arg(destination)
    };
    for mapping in &install.files {
      for file in FileSource::expand(&project.root, mapping, outcome.build_type)? {
        if file.config && file.destination.exists() {
          shell.status("Preserved", file.destination.display())?;
          continue;
        }
        privileged.run(
          shell,
          &install_file(&file.source, &file.destination, file.mode.to_string()),
        )?;
      }
    }
    for tmpfile in &install.tmpfiles {
      let destination = Self::tmpfiles_path(tmpfile);
      privileged.run(
        shell,
        &install_file(&project.join(tmpfile), &destination, "0644".to_owned()),
      )?;
      if which::which("systemd-tmpfiles").is_ok() {
        privileged.run(
          shell,
          &privileged
            .command("systemd-tmpfiles")
            .arg("--create")
            .arg(&destination),
        )?;
      }
    }
    for (service, unit) in install.services.iter().zip(self.units()) {
      privileged.run(
        shell,
        &install_file(&project.join(service), &unit, "0644".to_owned()),
      )?;
    }
    if !install.services.is_empty() && which::which("systemctl").is_ok() {
      privileged.run(shell, &privileged.command("systemctl").arg("daemon-reload"))?;
      let names = install.services.iter().filter_map(|s| s.file_name());
      privileged.run(
        shell,
        &privileged
          .command("systemctl")
          .args(["enable", "--now"])
          .args(names),
      )?;
    }
    Ok(())
  }

  fn remove_packages(&self, shell: &mut Shell) -> Result<bool> {
    let privileged = self.privileged;
    let installed: Vec<String> = match self.distro {
      Distro::Debian => self
        .package_names()
        .into_iter()
        .filter(|name| {
          ProcessBuilder::new("dpkg-query")
            .args(["-W", "-f=${db:Status-Status}", name])
            .output_unchecked()
            .is_ok_and(|o| o.stdout == b"installed")
        })
        .collect(),
      Distro::Arch => self
        .package_names()
        .into_iter()
        .filter(|name| {
          ProcessBuilder::new("pacman")
            .args(["-Qq", name])
            .output_unchecked()
            .is_ok_and(|o| o.status.success())
        })
        .collect(),
      Distro::Other(_) => Vec::new(),
    };
    if installed.is_empty() {
      return Ok(false);
    }
    let remove = match self.distro {
      Distro::Debian => privileged.command("apt-get").args(["remove", "-y"]),
      _ => privileged.command("pacman").args(["-R", "--noconfirm"]),
    };
    privileged.run(shell, &remove.args(&installed))?;
    Ok(true)
  }

  fn package_names(&self) -> Vec<String> {
    if self.distro == Distro::Arch
      && let Ok(info) = ArchPackager::new(self.packaging).and_then(|a| a.src_info())
      && !info.pkgnames.is_empty()
    {
      return info.pkgnames;
    }
    vec![self.packaging.name().to_owned()]
  }

  fn units(&self) -> Vec<std::path::PathBuf> {
    self
      .packaging
      .project()
      .manifest
      .install
      .services
      .iter()
      .filter_map(|service| service.file_name())
      .map(|name| std::path::Path::new(Self::UNIT_DIR).join(name))
      .collect()
  }

  fn tmpfiles_path(tmpfile: &std::path::Path) -> std::path::PathBuf {
    let stem = tmpfile.file_stem().unwrap_or_default().to_string_lossy();
    std::path::Path::new(Self::TMPFILES_DIR).join(format!("{stem}.conf"))
  }
}
