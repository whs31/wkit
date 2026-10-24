use std::path::{Path, PathBuf};

use super::{Distro, FileSource, Packaging, Privileged, arch::Pkgbuild, deb::deb};
use crate::{ProcessBuilder, Result, Shell, build::Builder, hint};

const TMPFILES_DIR: &str = "/usr/lib/tmpfiles.d";
const UNIT_DIR: &str = "/usr/lib/systemd/system";

#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
enum Method {
  #[display("Debian package")]
  Deb,
  #[display("Arch package")]
  Arch,
  #[display("files")]
  Files,
}

pub fn install(
  packaging: &Packaging<'_>,
  shell: &mut Shell,
  dry_run: bool,
  files: bool,
) -> Result<()> {
  Installer::new(packaging, dry_run).install(shell, files)
}

pub fn uninstall(packaging: &Packaging<'_>, shell: &mut Shell, dry_run: bool) -> Result<()> {
  Installer::new(packaging, dry_run).uninstall(shell)
}

#[derive(Debug, Clone)]
struct Installer<'p> {
  packaging: &'p Packaging<'p>,
  privileged: Privileged,
  distro: Distro,
}

impl<'p> Installer<'p> {
  fn new(packaging: &'p Packaging<'p>, dry_run: bool) -> Self {
    Self {
      packaging,
      privileged: Privileged::new(dry_run),
      distro: Distro::detect(),
    }
  }

  fn method(&self, files: bool) -> Result<Method> {
    let manifest = &self.packaging.project.manifest;
    let has_files = !manifest.install.is_empty();
    if files {
      anyhow::ensure!(
        has_files,
        "`--files` needs an `[install]` section in wkit.toml"
      );
      return Ok(Method::Files);
    }
    match self.distro {
      Distro::Debian if manifest.package.deb.is_some() || self.packaging.is_cargo_project() => {
        Ok(Method::Deb)
      }
      Distro::Arch if Pkgbuild::locate(self.packaging).is_ok() => Ok(Method::Arch),
      _ if has_files => Ok(Method::Files),
      _ => Err(hint(
        format!("nothing to install on {}", self.distro),
        "add `[package.deb]`, a PKGBUILD or an `[install]` section to wkit.toml",
      )),
    }
  }

  fn install(&self, shell: &mut Shell, files: bool) -> Result<()> {
    let method = self.method(files)?;
    let privileged = self.privileged;
    match method {
      Method::Deb => {
        let deb = deb(self.packaging, shell, &[])?;
        privileged.run(
          shell,
          &privileged
            .command("apt-get")
            .args(["install", "-y", "--reinstall"])
            .arg(deb),
        )?;
      }
      Method::Arch => {
        let packages: Vec<_> = Pkgbuild::locate(self.packaging)?
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
      Method::Files => self.install_files(shell)?,
    }
    if !privileged.dry_run {
      shell.status(
        "Installed",
        format_args!("{} ({method})", self.packaging.name),
      )?;
    }
    Ok(())
  }

  fn uninstall(&self, shell: &mut Shell) -> Result<()> {
    let privileged = self.privileged;
    let mut removed = self.remove_packages(shell)?;
    let install = &self.packaging.project.manifest.install;
    let systemctl = which::which("systemctl").is_ok();
    if systemctl {
      for unit in self.units().iter().filter(|unit| unit.exists()) {
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
        self.packaging.name
      ))?;
    } else if !privileged.dry_run {
      shell.status("Removed", &self.packaging.name)?;
    }
    Ok(())
  }

  fn installed_paths(&self) -> Vec<PathBuf> {
    let project = self.packaging.project;
    let install = &project.manifest.install;
    let build_type = self.packaging.build_type();
    let mut paths: Vec<PathBuf> = install.prefix.iter().cloned().collect();
    for mapping in install.files.iter().filter(|m| !m.config) {
      if mapping.is_glob() {
        let found = FileSource::expand(&project.root, mapping, build_type).unwrap_or_default();
        paths.extend(found.into_iter().map(|f| f.destination));
      } else {
        paths.push(mapping.destination(Path::new(&mapping.source(build_type))));
      }
    }
    paths.extend(install.tmpfiles.iter().map(|t| tmpfiles_path(t)));
    paths.extend(self.units());
    paths
  }

  fn install_files(&self, shell: &mut Shell) -> Result<()> {
    let packaging = self.packaging;
    let project = packaging.project;
    let install = &project.manifest.install;
    let privileged = self.privileged;
    let outcome = Builder::new(project, &packaging.options).run(shell)?;
    if let Some(prefix) = &install.prefix {
      let build_dir = outcome
        .build_dir()
        .ok_or_else(|| anyhow::anyhow!("`install.prefix` needs a CMake build"))?;
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
    let install_file = |source: &Path, destination: &Path, mode: &str| {
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
          &install_file(&file.source, &file.destination, &file.mode.to_string()),
        )?;
      }
    }
    for tmpfile in &install.tmpfiles {
      let destination = tmpfiles_path(tmpfile);
      privileged.run(
        shell,
        &install_file(&project.join(tmpfile), &destination, "0644"),
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
      privileged.run(shell, &install_file(&project.join(service), &unit, "0644"))?;
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
    let installed = |probe: fn(&str) -> ProcessBuilder,
                     success: fn(&std::process::Output) -> bool|
     -> Vec<String> {
      self
        .package_names()
        .into_iter()
        .filter(|name| probe(name).output_unchecked().is_ok_and(|o| success(&o)))
        .collect()
    };
    let (installed, remove) = match self.distro {
      Distro::Debian => (
        installed(
          |name| ProcessBuilder::new("dpkg-query").args(["-W", "-f=${db:Status-Status}", name]),
          |o| o.stdout == b"installed",
        ),
        self.privileged.command("apt-get").args(["remove", "-y"]),
      ),
      Distro::Arch => (
        installed(
          |name| ProcessBuilder::new("pacman").args(["-Qq", name]),
          |o| o.status.success(),
        ),
        self
          .privileged
          .command("pacman")
          .args(["-R", "--noconfirm"]),
      ),
      Distro::Other(_) => return Ok(false),
    };
    if installed.is_empty() {
      return Ok(false);
    }
    self.privileged.run(shell, &remove.args(&installed))?;
    Ok(true)
  }

  fn package_names(&self) -> Vec<String> {
    if self.distro == Distro::Arch
      && let Ok(info) = Pkgbuild::locate(self.packaging).and_then(|a| a.src_info())
      && !info.pkgnames.is_empty()
    {
      return info.pkgnames;
    }
    vec![self.packaging.name.clone()]
  }

  fn units(&self) -> Vec<PathBuf> {
    let services = &self.packaging.project.manifest.install.services;
    services
      .iter()
      .filter_map(|service| service.file_name())
      .map(|name| Path::new(UNIT_DIR).join(name))
      .collect()
  }
}

fn tmpfiles_path(tmpfile: &Path) -> PathBuf {
  let stem = tmpfile.file_stem().unwrap_or_default().to_string_lossy();
  Path::new(TMPFILES_DIR).join(format!("{stem}.conf"))
}
