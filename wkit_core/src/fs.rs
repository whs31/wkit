mod source_set;

pub use self::source_set::SourceSet;
use crate::{Error, Result};

pub fn find_upwards(start: &std::path::Path, file_name: &str) -> Option<std::path::PathBuf> {
  start
    .ancestors()
    .map(|dir| dir.join(file_name))
    .find(|candidate| candidate.is_file())
}

pub fn read_to_string(path: &std::path::Path) -> Result<String> {
  std::fs::read_to_string(path).map_err(|source| Error::Read {
    path: path.to_owned(),
    source,
  })
}

pub fn read_optional(path: &std::path::Path) -> Result<Option<String>> {
  match std::fs::read_to_string(path) {
    Ok(text) => Ok(Some(text)),
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
    Err(source) => Err(Error::Read {
      path: path.to_owned(),
      source,
    }),
  }
}

pub fn write(path: &std::path::Path, contents: impl AsRef<[u8]>) -> Result<()> {
  std::fs::write(path, contents).map_err(|source| Error::Write {
    path: path.to_owned(),
    source,
  })
}

pub fn find_executable(dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
  let exe = format!("{name}{}", std::env::consts::EXE_SUFFIX);
  let direct = dir.join(&exe);
  if direct.is_file() {
    return Some(direct);
  }
  let file_name = std::path::Path::new(&exe).file_name()?.to_owned();
  let mut pending = vec![dir.to_owned()];
  let mut found = Vec::new();
  while let Some(current) = pending.pop() {
    for entry in std::fs::read_dir(&current).into_iter().flatten().flatten() {
      let path = entry.path();
      let Ok(kind) = entry.file_type() else {
        continue;
      };
      if kind.is_dir() && entry.file_name() != "CMakeFiles" {
        pending.push(path);
      } else if kind.is_file() && entry.file_name() == file_name {
        found.push(path);
      }
    }
  }
  found.sort();
  found.into_iter().next()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn finds_file_in_ancestors() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("wkit.toml"), "").unwrap();
    let nested = dir.path().join("a/b");
    std::fs::create_dir_all(&nested).unwrap();
    assert_eq!(
      find_upwards(&nested, "wkit.toml"),
      Some(dir.path().join("wkit.toml"))
    );
    assert_eq!(find_upwards(&nested, "nope.toml"), None);
  }

  #[test]
  fn finds_executables_outside_cmake_internals() {
    let dir = tempfile::tempdir().unwrap();
    let exe = |rel: &str| {
      dir
        .path()
        .join(format!("{rel}{}", std::env::consts::EXE_SUFFIX))
    };
    for rel in ["CMakeFiles/app", "tools/app", "z/app"] {
      std::fs::create_dir_all(exe(rel).parent().unwrap()).unwrap();
      std::fs::write(exe(rel), "").unwrap();
    }
    assert_eq!(find_executable(dir.path(), "app"), Some(exe("tools/app")));
    std::fs::write(exe("app"), "").unwrap();
    assert_eq!(find_executable(dir.path(), "app"), Some(exe("app")));
    assert_eq!(find_executable(dir.path(), "nope"), None);
  }

  #[test]
  fn optional_read_distinguishes_missing_from_failing() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(read_optional(&dir.path().join("missing")).unwrap(), None);
    assert!(matches!(read_optional(dir.path()), Err(Error::Read { .. })));
    write(&dir.path().join("x"), "1").unwrap();
    assert_eq!(read_to_string(&dir.path().join("x")).unwrap(), "1");
  }
}
