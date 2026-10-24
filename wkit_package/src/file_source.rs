use wkit_manifest::{BuildType, FileMapping, FileMode};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSource {
  pub source: std::path::PathBuf,
  pub destination: std::path::PathBuf,
  pub mode: FileMode,
  pub config: bool,
}

impl FileSource {
  pub fn expand(
    root: &std::path::Path,
    mapping: &FileMapping,
    build_type: BuildType,
  ) -> Result<Vec<Self>> {
    let pattern = mapping.source(build_type);
    let sources = if mapping.is_glob() {
      Self::glob(root, &pattern)?
    } else {
      let source = root.join(&pattern);
      if !source.is_file() {
        return Err(Error::setup(
          format!("`{pattern}` does not exist"),
          Some("check `from` in the `files` mapping".to_owned()),
        ));
      }
      vec![source]
    };
    Ok(
      sources
        .into_iter()
        .map(|source| Self {
          destination: mapping.destination(&source),
          mode: mapping.mode.unwrap_or_else(|| Self::default_mode(&source)),
          config: mapping.config,
          source,
        })
        .collect(),
    )
  }

  pub fn default_mode(path: &std::path::Path) -> FileMode {
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      if std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0) {
        return FileMode::EXECUTABLE;
      }
    }
    #[cfg(not(unix))]
    let _ = path;
    FileMode::REGULAR
  }

  fn glob(root: &std::path::Path, pattern: &str) -> Result<Vec<std::path::PathBuf>> {
    let (dir, name) = pattern.rsplit_once('/').unwrap_or((".", pattern));
    let matcher = globset::Glob::new(name)
      .map_err(|e| Error::setup(format!("invalid pattern `{pattern}`: {e}"), None))?
      .compile_matcher();
    let mut found: Vec<_> = std::fs::read_dir(root.join(dir))
      .into_iter()
      .flatten()
      .flatten()
      .filter(|entry| {
        entry.file_type().is_ok_and(|t| t.is_file()) && matcher.is_match(entry.file_name())
      })
      .map(|entry| entry.path())
      .collect();
    if found.is_empty() {
      return Err(Error::setup(format!("`{pattern}` matches no files"), None));
    }
    found.sort();
    Ok(found)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn mapping(from: &str, to: &str) -> FileMapping {
    FileMapping {
      from: from.into(),
      to: to.into(),
      mode: None,
      config: false,
    }
  }

  #[test]
  fn globs_expand_into_directories() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("lua")).unwrap();
    for name in ["b.lua", "a.lua", "notes.txt"] {
      std::fs::write(dir.path().join("lua").join(name), "").unwrap();
    }
    let found = FileSource::expand(
      dir.path(),
      &mapping("lua/*.lua", "/etc/relay/lua/"),
      BuildType::Release,
    )
    .unwrap();
    let destinations: Vec<_> = found.iter().map(|f| f.destination.clone()).collect();
    assert_eq!(
      destinations,
      [
        std::path::PathBuf::from("/etc/relay/lua/a.lua"),
        "/etc/relay/lua/b.lua".into()
      ]
    );
    assert_eq!(found[0].mode, FileMode::REGULAR);
    let err = FileSource::expand(
      dir.path(),
      &mapping("lua/*.so", "/usr/lib/"),
      BuildType::Release,
    )
    .unwrap_err();
    assert!(err.to_string().contains("matches no files"), "{err}");
    let err = FileSource::expand(
      dir.path(),
      &mapping("missing", "/usr/bin/x"),
      BuildType::Release,
    )
    .unwrap_err();
    assert!(
      err.to_string().contains("`missing` does not exist"),
      "{err}"
    );
  }
}
