use wkit_core::{ProcessBuilder, Shell, version::Flavor};
use wkit_manifest::package::ArchPackage;

use crate::{Error, Packaging, Privileged, Result, SourceTarball, SrcInfo, ToolRun};

#[derive(Debug, Clone)]
pub struct ArchPackager<'p> {
  packaging: &'p Packaging<'p>,
  dir: std::path::PathBuf,
}

impl<'p> ArchPackager<'p> {
  pub fn new(packaging: &'p Packaging<'p>) -> Result<Self> {
    let project = packaging.project();
    let dir = project
      .manifest
      .package
      .arch
      .locate(&project.root)
      .filter(|dir| dir.join(ArchPackage::PKGBUILD).is_file())
      .ok_or_else(|| {
        Error::setup(
          "no PKGBUILD found",
          Some(format!(
            "put it in {} or set `package.arch.dir`",
            ArchPackage::DEFAULT_DIRS.join(" or ")
          )),
        )
      })?;
    Ok(Self { packaging, dir })
  }

  pub fn is_available(packaging: &'p Packaging<'p>) -> bool {
    Self::new(packaging).is_ok()
  }

  pub fn src_info(&self) -> Result<SrcInfo> {
    let output = self.makepkg()?.arg("--printsrcinfo").output()?;
    Ok(SrcInfo::parse(&String::from_utf8_lossy(&output.stdout)))
  }

  pub fn run(
    &self,
    shell: &mut Shell,
    privileged: Privileged,
    extra: &[String],
  ) -> Result<Vec<std::path::PathBuf>> {
    let packaging = self.packaging;
    let project = packaging.project();
    let info = self.src_info()?;
    self.check_version(&info)?;
    Self::install_missing(shell, privileged, &info)?;
    let mut archives = Vec::new();
    for (file, prefix) in info.local_archives() {
      let tarball = SourceTarball::collect(&project.root, &self.dir)?;
      let archive = self.dir.join(file);
      tarball.write(&project.root, prefix, &archive)?;
      shell.status(
        "Archiving",
        format_args!("{file} ({} files)", tarball.len()),
      )?;
      archives.push(archive);
    }
    let build = self
      .makepkg()?
      .args([
        "--syncdeps",
        "--noconfirm",
        "--cleanbuild",
        "--clean",
        "--force",
      ])
      .args(extra);
    ToolRun::new(shell, packaging.log_path())?.run(&build, "Packaging", |line| {
      line.strip_prefix("==> Making package: ").map(|rest| {
        (
          "Packaging",
          rest.split(" (").next().unwrap_or(rest).to_owned(),
        )
      })
    })?;
    for archive in archives {
      std::fs::remove_file(archive)?;
    }
    let list = self.makepkg()?.arg("--packagelist").output()?;
    let packages: Vec<std::path::PathBuf> = String::from_utf8_lossy(&list.stdout)
      .lines()
      .map(std::path::PathBuf::from)
      .filter(|path| path.is_file())
      .collect();
    for package in &packages {
      shell.status("Packaged", packaging.shown(package))?;
    }
    Ok(packages)
  }

  fn makepkg(&self) -> Result<ProcessBuilder> {
    if which::which("makepkg").is_err() {
      return Err(Error::setup(
        "Arch packages need makepkg",
        Some("run this on Arch Linux or a derivative".to_owned()),
      ));
    }
    let mut makepkg = self
      .packaging
      .with_env(ProcessBuilder::new("makepkg").cwd(&self.dir));
    if self.packaging.has_explicit_output() || std::env::var_os("PKGDEST").is_none() {
      std::fs::create_dir_all(self.packaging.output())?;
      makepkg = makepkg.env("PKGDEST", self.packaging.output());
    }
    Ok(makepkg)
  }

  fn check_version(&self, info: &SrcInfo) -> Result<()> {
    let expected = self.packaging.version()?.render(Flavor::PkgVer);
    let mismatches = self.packaging.mismatches()?;
    let root = &self.packaging.project().root;
    let pkgbuild = self.dir.join(ArchPackage::PKGBUILD);
    let reported = mismatches.iter().any(|m| root.join(&m.path) == pkgbuild);
    let mut problems: Vec<String> = mismatches.iter().map(ToString::to_string).collect();
    if info.pkgver != expected && !reported {
      problems.push(format!(
        "PKGBUILD has pkgver {}, expected {expected}",
        info.pkgver
      ));
    }
    if problems.is_empty() {
      return Ok(());
    }
    Err(Error::setup(
      format!("version mismatch:\n  {}", problems.join("\n  ")),
      Some("run `wkit bump` to see every version file".to_owned()),
    ))
  }

  fn install_missing(shell: &mut Shell, privileged: Privileged, info: &SrcInfo) -> Result<()> {
    if info.depends.is_empty() || which::which("pacman").is_err() {
      return Ok(());
    }
    let missing = ProcessBuilder::new("pacman")
      .arg("-T")
      .args(&info.depends)
      .output_unchecked()?;
    let missing: Vec<String> = String::from_utf8_lossy(&missing.stdout)
      .split_whitespace()
      .map(ToOwned::to_owned)
      .collect();
    if missing.is_empty() {
      return Ok(());
    }
    privileged.run(
      shell,
      &privileged
        .command("pacman")
        .args(["-S", "--needed", "--asdeps", "--noconfirm"])
        .args(&missing),
    )
  }
}
