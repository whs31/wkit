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
  fn optional_read_distinguishes_missing_from_failing() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(read_optional(&dir.path().join("missing")).unwrap(), None);
    assert!(matches!(read_optional(dir.path()), Err(Error::Read { .. })));
    write(&dir.path().join("x"), "1").unwrap();
    assert_eq!(read_to_string(&dir.path().join("x")).unwrap(), "1");
  }
}
