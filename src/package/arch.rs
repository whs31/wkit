use std::path::{Path, PathBuf};

use anyhow::Context as _;

use super::{Packaging, Privileged};
use crate::{ProcessBuilder, Result, Shell, hint, manifest::ArchPackage, version::Flavor};

const ARCHIVE_SUFFIXES: &[&str] = &[".tar.gz", ".tar.xz", ".tar.zst", ".tgz"];

pub fn arch(
  packaging: &Packaging<'_>,
  shell: &mut Shell,
  privileged: Privileged,
  extra: &[String],
) -> Result<Vec<PathBuf>> {
  Pkgbuild::locate(packaging)?.run(shell, privileged, extra)
}

#[derive(Debug, Clone)]
pub struct Pkgbuild<'p> {
  packaging: &'p Packaging<'p>,
  dir: PathBuf,
}

impl<'p> Pkgbuild<'p> {
  pub fn locate(packaging: &'p Packaging<'p>) -> Result<Self> {
    let project = packaging.project;
    let dir = project
      .manifest
      .package
      .arch
      .locate(&project.root)
      .filter(|dir| dir.join("PKGBUILD").is_file())
      .ok_or_else(|| {
        hint(
          "no PKGBUILD found",
          format!(
            "put it in {} or set `package.arch.dir`",
            ArchPackage::DEFAULT_DIRS.join(" or ")
          ),
        )
      })?;
    Ok(Self { packaging, dir })
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
  ) -> Result<Vec<PathBuf>> {
    let packaging = self.packaging;
    let root = &packaging.project.root;
    let info = self.src_info()?;
    self.check_version(&info)?;
    Self::install_missing(shell, privileged, &info)?;
    let mut archives = Vec::new();
    for (file, prefix) in info.local_archives() {
      let files = source_files(root, &self.dir)?;
      let archive = self.dir.join(file);
      write_tarball(root, &files, prefix, &archive)?;
      shell.status("Archiving", format_args!("{file} ({} files)", files.len()))?;
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
    packaging.run_tool(shell, &build, "Packaging", |line| {
      line.strip_prefix("==> Making package: ").map(|rest| {
        (
          "Packaging",
          rest.split(" (").next().unwrap_or(rest).to_owned(),
        )
      })
    })?;
    for archive in archives {
      fs_err::remove_file(archive)?;
    }
    let list = self.makepkg()?.arg("--packagelist").output()?;
    let packages: Vec<PathBuf> = String::from_utf8_lossy(&list.stdout)
      .lines()
      .map(PathBuf::from)
      .filter(|path| path.is_file())
      .collect();
    for package in &packages {
      shell.status("Packaged", packaging.shown(package))?;
    }
    Ok(packages)
  }

  fn makepkg(&self) -> Result<ProcessBuilder> {
    if which::which("makepkg").is_err() {
      return Err(hint(
        "Arch packages need makepkg",
        "run this on Arch Linux or a derivative",
      ));
    }
    let packaging = self.packaging;
    let mut makepkg = packaging.with_env(ProcessBuilder::new("makepkg").cwd(&self.dir));
    if packaging.explicit_output || std::env::var_os("PKGDEST").is_none() {
      fs_err::create_dir_all(&packaging.output)?;
      makepkg = makepkg.env("PKGDEST", &packaging.output);
    }
    Ok(makepkg)
  }

  fn check_version(&self, info: &SrcInfo) -> Result<()> {
    let expected = self.packaging.version()?.render(Flavor::PkgVer);
    let mismatches = self.packaging.mismatches()?;
    let root = &self.packaging.project.root;
    let pkgbuild = self.dir.join("PKGBUILD");
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
    Err(hint(
      format!("version mismatch:\n  {}", problems.join("\n  ")),
      "run `wkit bump` to see every version file",
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
    let missing: Vec<_> = String::from_utf8_lossy(&missing.stdout)
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SrcInfo {
  pub pkgver: String,
  pub pkgnames: Vec<String>,
  pub sources: Vec<String>,
  pub depends: Vec<String>,
}

impl SrcInfo {
  fn parse(text: &str) -> Self {
    let mut info = Self::default();
    let mut in_base = true;
    for line in text.lines() {
      let Some((key, value)) = line.split_once(" = ") else {
        continue;
      };
      let value = value.trim().to_owned();
      match key.trim() {
        "pkgname" => {
          in_base = false;
          info.pkgnames.push(value);
        }
        "pkgver" => info.pkgver = value,
        "source" if in_base => info.sources.push(value),
        "depends" | "makedepends" if in_base => info.depends.push(value),
        _ => {}
      }
    }
    info
  }

  fn local_archives(&self) -> impl Iterator<Item = (&str, &str)> {
    self
      .sources
      .iter()
      .filter(|source| !source.contains("://"))
      .filter_map(|source| {
        let file = source
          .split_once("::")
          .map_or(source.as_str(), |(name, _)| name);
        ARCHIVE_SUFFIXES
          .iter()
          .find_map(|suffix| file.strip_suffix(suffix))
          .map(|prefix| (file, prefix))
      })
  }
}

fn source_files(root: &Path, skip: &Path) -> Result<Vec<PathBuf>> {
  let output = ProcessBuilder::new("git")
    .cwd(root)
    .args([
      "ls-files",
      "-z",
      "--cached",
      "--others",
      "--exclude-standard",
    ])
    .output()
    .context("the source archive is built from git, but this is not a git checkout")?;
  let skip = skip.strip_prefix(root).unwrap_or(skip);
  let mut files: Vec<_> = output
    .stdout
    .split(|&b| b == 0)
    .filter(|name| !name.is_empty())
    .map(|name| PathBuf::from(String::from_utf8_lossy(name).into_owned()))
    .filter(|path| !is_package_output(path, skip))
    .filter(|path| std::fs::symlink_metadata(root.join(path)).is_ok())
    .collect();
  files.sort();
  files.dedup();
  Ok(files)
}

fn write_tarball(root: &Path, files: &[PathBuf], prefix: &str, output: &Path) -> Result<()> {
  let file = std::io::BufWriter::new(fs_err::File::create(output)?);
  let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
    file,
    flate2::Compression::default(),
  ));
  tar.follow_symlinks(false);
  for path in files {
    tar.append_path_with_name(root.join(path), Path::new(prefix).join(path))?;
  }
  tar.into_inner()?.finish()?;
  Ok(())
}

fn is_package_output(path: &Path, skip: &Path) -> bool {
  let Ok(inside) = path.strip_prefix(skip) else {
    return false;
  };
  let name = inside.to_string_lossy();
  let first = inside.components().next().map(|c| c.as_os_str().to_owned());
  name.contains(".pkg.tar")
    || ARCHIVE_SUFFIXES.iter().any(|s| name.ends_with(s))
    || first.is_some_and(|c| c == "src" || c == "pkg") && inside.components().count() > 1
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reads_versions_names_and_local_archives() {
    let info = SrcInfo::parse(
      "pkgbase = mmsc\n\tpkgdesc = MMS\n\tpkgver = 3.12.0rc.1\n\tpkgrel = 1\n\tmakedepends = cmake\n\tdepends = glibc\n\tsource = mms-3.12.0-rc.1.tar.gz\n\tsource = https://example.com/x.tar.gz\n\tsource = mmsc.install\n\npkgname = mmsc\n\tdepends = qt6-base\n",
    );
    assert_eq!(info.pkgver, "3.12.0rc.1");
    assert_eq!(info.pkgnames, ["mmsc"]);
    assert_eq!(info.depends, ["cmake", "glibc"]);
    assert_eq!(
      info.local_archives().collect::<Vec<_>>(),
      [("mms-3.12.0-rc.1.tar.gz", "mms-3.12.0-rc.1")]
    );
  }

  #[test]
  fn package_outputs_next_to_the_pkgbuild_are_skipped() {
    let skip = Path::new("packaging/arch");
    for (path, skipped) in [
      ("packaging/arch/PKGBUILD", false),
      ("packaging/arch/x.install", false),
      ("packaging/arch/x-1.0.tar.gz", true),
      ("packaging/arch/x-1.0-1-x86_64.pkg.tar.zst", true),
      ("packaging/arch/src/x/main.c", true),
      ("src/main.c", false),
      ("docs/x.tar.gz", false),
    ] {
      assert_eq!(is_package_output(Path::new(path), skip), skipped, "{path}");
    }
  }

  #[test]
  fn archives_git_files_under_a_prefix() {
    if which::which("git").is_err() {
      return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    ProcessBuilder::new("git")
      .cwd(root)
      .args(["init", "-q"])
      .output()
      .unwrap();
    std::fs::write(root.join(".gitignore"), "build/\n").unwrap();
    std::fs::create_dir_all(root.join("build")).unwrap();
    std::fs::write(root.join("build/out"), "").unwrap();
    std::fs::write(root.join("main.c"), "int main(){}\n").unwrap();
    let files = source_files(root, &root.join("packaging/arch")).unwrap();
    assert_eq!(files.len(), 2);
    let out = root.join("x.tar.gz");
    write_tarball(root, &files, "x-1.0", &out).unwrap();
    let names: Vec<_> = tar::Archive::new(flate2::read::GzDecoder::new(
      std::fs::File::open(&out).unwrap(),
    ))
    .entries()
    .unwrap()
    .map(|e| e.unwrap().path().unwrap().display().to_string())
    .collect();
    assert_eq!(names, ["x-1.0/.gitignore", "x-1.0/main.c"]);
  }
}
