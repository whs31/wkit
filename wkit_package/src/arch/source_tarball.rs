use wkit_core::ProcessBuilder;

use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct SourceTarball {
  files: Vec<std::path::PathBuf>,
}

impl SourceTarball {
  pub fn collect(root: &std::path::Path, skip: &std::path::Path) -> Result<Self> {
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
      .map_err(|_| {
        Error::setup(
          "the source archive is built from git, but this is not a git checkout",
          None,
        )
      })?;
    let skip = skip.strip_prefix(root).unwrap_or(skip);
    let mut files: Vec<_> = output
      .stdout
      .split(|&b| b == 0)
      .filter(|name| !name.is_empty())
      .map(|name| std::path::PathBuf::from(String::from_utf8_lossy(name).into_owned()))
      .filter(|path| !Self::is_output(path, skip))
      .filter(|path| std::fs::symlink_metadata(root.join(path)).is_ok())
      .collect();
    files.sort();
    files.dedup();
    Ok(Self { files })
  }

  pub fn len(&self) -> usize {
    self.files.len()
  }

  pub fn is_empty(&self) -> bool {
    self.files.is_empty()
  }

  pub fn write(
    &self,
    root: &std::path::Path,
    prefix: &str,
    output: &std::path::Path,
  ) -> Result<()> {
    let file = std::fs::File::create(output).map_err(|source| wkit_core::Error::Write {
      path: output.to_owned(),
      source,
    })?;
    let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
      std::io::BufWriter::new(file),
      flate2::Compression::default(),
    ));
    tar.follow_symlinks(false);
    for path in &self.files {
      tar.append_path_with_name(root.join(path), std::path::Path::new(prefix).join(path))?;
    }
    tar.into_inner()?.finish()?;
    Ok(())
  }

  fn is_output(path: &std::path::Path, skip: &std::path::Path) -> bool {
    let Ok(inside) = path.strip_prefix(skip) else {
      return false;
    };
    let name = inside.to_string_lossy();
    let first = inside.components().next().map(|c| c.as_os_str().to_owned());
    name.contains(".pkg.tar")
      || crate::SrcInfo::ARCHIVE_SUFFIXES
        .iter()
        .any(|s| name.ends_with(s))
      || first.is_some_and(|c| c == "src" || c == "pkg") && inside.components().count() > 1
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn package_outputs_next_to_the_pkgbuild_are_skipped() {
    let skip = std::path::Path::new("packaging/arch");
    for (path, skipped) in [
      ("packaging/arch/PKGBUILD", false),
      ("packaging/arch/x.install", false),
      ("packaging/arch/x-1.0.tar.gz", true),
      ("packaging/arch/x-1.0-1-x86_64.pkg.tar.zst", true),
      ("packaging/arch/src/x/main.c", true),
      ("src/main.c", false),
      ("docs/x.tar.gz", false),
    ] {
      assert_eq!(
        SourceTarball::is_output(std::path::Path::new(path), skip),
        skipped,
        "{path}"
      );
    }
  }

  #[test]
  fn archives_git_files_under_a_prefix() {
    if which::which("git").is_err() {
      return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let git = |args: &[&str]| {
      ProcessBuilder::new("git")
        .cwd(root)
        .args(args)
        .output()
        .unwrap()
    };
    git(&["init", "-q"]);
    std::fs::write(root.join(".gitignore"), "build/\n").unwrap();
    std::fs::create_dir_all(root.join("build")).unwrap();
    std::fs::write(root.join("build/out"), "").unwrap();
    std::fs::write(root.join("main.c"), "int main(){}\n").unwrap();
    let tarball = SourceTarball::collect(root, &root.join("packaging/arch")).unwrap();
    assert_eq!(tarball.len(), 2);
    let out = root.join("x.tar.gz");
    tarball.write(root, "x-1.0", &out).unwrap();
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
