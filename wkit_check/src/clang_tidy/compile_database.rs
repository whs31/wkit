use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct CompileDatabase {
  entries: Vec<serde_json::Map<String, serde_json::Value>>,
}

impl CompileDatabase {
  pub const FILE: &str = "compile_commands.json";

  pub fn discover(
    root: &std::path::Path,
    explicit: Option<&std::path::Path>,
  ) -> Result<std::path::PathBuf> {
    if let Some(dir) = explicit {
      let path = root.join(dir).join(Self::FILE);
      return if path.is_file() {
        Ok(path)
      } else {
        Err(Error::Message(format!(
          "`{}` does not exist",
          path.display()
        )))
      };
    }
    let mut candidates = vec![root.join(Self::FILE), root.join("build").join(Self::FILE)];
    for parent in [root.join("build"), root.to_owned()] {
      let Ok(entries) = std::fs::read_dir(&parent) else {
        continue;
      };
      for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if parent != root || name.starts_with("cmake-build-") {
          candidates.push(entry.path().join(Self::FILE));
        }
      }
    }
    candidates
      .into_iter()
      .filter_map(|path| {
        let modified = std::fs::metadata(&path).ok()?.modified().ok()?;
        Some((modified, path))
      })
      .max()
      .map(|(_, path)| path)
      .ok_or_else(|| {
        Error::Message("no compile_commands.json found in build/*/; pass BUILD_DIR".to_owned())
      })
  }

  pub fn load(path: &std::path::Path) -> Result<Self> {
    let text = wkit_core::fs::read_to_string(path)?;
    let entries = serde_json::from_str(&text).map_err(|source| Error::Json {
      path: path.to_owned(),
      source,
    })?;
    Ok(Self { entries })
  }

  pub fn strip(&mut self, flags: &[String]) {
    for entry in &mut self.entries {
      if let Some(serde_json::Value::String(command)) = entry.get_mut("command") {
        for flag in flags {
          *command = command.replace(&format!(" {flag}"), "");
        }
      }
      if let Some(serde_json::Value::Array(arguments)) = entry.get_mut("arguments") {
        arguments.retain(|a| !a.as_str().is_some_and(|a| flags.iter().any(|f| f == a)));
      }
    }
  }

  pub fn write_to(&self, dir: &std::path::Path) -> Result<()> {
    let json = serde_json::to_string(&self.entries).expect("entries serialize");
    wkit_core::fs::write(&dir.join(Self::FILE), json).map_err(Into::into)
  }

  pub fn sources(
    &self,
  ) -> std::collections::BTreeMap<std::path::PathBuf, (std::path::PathBuf, String)> {
    let mut sources = std::collections::BTreeMap::new();
    for entry in &self.entries {
      let (Some(directory), Some(file)) = (
        entry.get("directory").and_then(serde_json::Value::as_str),
        entry.get("file").and_then(serde_json::Value::as_str),
      ) else {
        continue;
      };
      let directory = std::path::PathBuf::from(directory);
      let command = serde_json::to_string(entry).expect("entries serialize");
      sources
        .entry(directory.join(file))
        .or_insert((directory, command));
    }
    sources
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn strips_flags_and_lists_first_commands() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CompileDatabase::FILE);
    std::fs::write(
      &path,
      r#"[
        {"directory": "/b", "file": "/src/a.cpp", "command": "c++ -mno-direct-extern-access -c /src/a.cpp"},
        {"directory": "/b", "file": "/src/a.cpp", "command": "c++ -DSECOND -c /src/a.cpp"},
        {"directory": "/b", "file": "rel.cpp", "arguments": ["c++", "-mno-direct-extern-access", "-c", "rel.cpp"]}
      ]"#,
    )
    .unwrap();
    let mut db = CompileDatabase::load(&path).unwrap();
    db.strip(&["-mno-direct-extern-access".to_owned()]);
    let sources = db.sources();
    assert_eq!(sources.len(), 2);
    let (dir_a, a) = &sources[std::path::Path::new("/src/a.cpp")];
    assert_eq!(dir_a, std::path::Path::new("/b"));
    assert!(a.contains("c++ -c /src/a.cpp") && !a.contains("SECOND"));
    assert!(
      !sources[std::path::Path::new("/b/rel.cpp")]
        .1
        .contains("extern-access")
    );
  }

  #[test]
  fn discovers_the_newest_database() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    assert!(CompileDatabase::discover(root, None).is_err());
    for sub in ["build/Debug", "build/Release"] {
      std::fs::create_dir_all(root.join(sub)).unwrap();
      std::fs::write(root.join(sub).join(CompileDatabase::FILE), "[]").unwrap();
      std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(
      CompileDatabase::discover(root, None).unwrap(),
      root.join("build/Release").join(CompileDatabase::FILE)
    );
    assert_eq!(
      CompileDatabase::discover(root, Some(std::path::Path::new("build/Debug"))).unwrap(),
      root.join("build/Debug").join(CompileDatabase::FILE)
    );
    assert!(CompileDatabase::discover(root, Some(std::path::Path::new("nope"))).is_err());
  }
}
