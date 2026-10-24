use super::{cache_entry::CacheEntry, tidy_run::TidyRun};
use crate::{Digest, Result};

#[derive(Debug)]
pub struct TidyCache {
  dir: std::path::PathBuf,
  setup: String,
  started: std::time::SystemTime,
  digests: std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, Option<Digest>>>,
}

impl TidyCache {
  pub const VERSION: u32 = 1;

  pub fn new(
    dir: std::path::PathBuf,
    setup: String,
    started: std::time::SystemTime,
  ) -> Result<Self> {
    std::fs::create_dir_all(&dir).map_err(|source| wkit_core::Error::Write {
      path: dir.clone(),
      source,
    })?;
    Ok(Self {
      dir,
      setup,
      started,
      digests: std::sync::Mutex::default(),
    })
  }

  pub fn entry_path(&self, source: &std::path::Path) -> std::path::PathBuf {
    let id = Digest::of(source.as_os_str().as_encoded_bytes());
    self.dir.join(format!("{id}.json"))
  }

  pub fn lookup(&self, source: &std::path::Path, command: &str) -> Option<TidyRun> {
    let text = std::fs::read_to_string(self.entry_path(source)).ok()?;
    let entry: CacheEntry = serde_json::from_str(&text).ok()?;
    if entry.key != self.key(command) {
      return None;
    }
    let fresh = entry
      .inputs
      .iter()
      .all(|(path, digest)| self.digest(path).is_some_and(|d| d.to_string() == *digest));
    fresh.then(|| TidyRun {
      source: source.to_owned(),
      success: entry.success,
      output: entry.output,
      cached: true,
    })
  }

  pub fn store(&self, run: &TidyRun, command: &str, inputs: &[std::path::PathBuf]) {
    let mut digests = std::collections::BTreeMap::new();
    for path in inputs {
      let Some(digest) = self.digest(path) else {
        return;
      };
      if self.modified_since_start(path) {
        return;
      }
      digests.insert(path.clone(), digest.to_string());
    }
    let entry = CacheEntry {
      key: self.key(command),
      inputs: digests,
      success: run.success,
      output: run.output.clone(),
    };
    let path = self.entry_path(&run.source);
    let temporary = path.with_extension("tmp");
    let json = serde_json::to_string(&entry).expect("entries serialize");
    if std::fs::write(&temporary, json).is_ok() {
      let _ = std::fs::rename(&temporary, &path);
    }
  }

  pub fn prune<'a>(&self, sources: impl IntoIterator<Item = &'a std::path::PathBuf>) {
    let keep: std::collections::HashSet<_> = sources
      .into_iter()
      .filter_map(|s| self.entry_path(s).file_name().map(ToOwned::to_owned))
      .collect();
    let Ok(entries) = std::fs::read_dir(&self.dir) else {
      return;
    };
    for entry in entries.flatten() {
      if !keep.contains(&entry.file_name()) {
        let _ = std::fs::remove_file(entry.path());
      }
    }
  }

  fn key(&self, command: &str) -> String {
    Digest::of(format!("{}\n{}\n{command}", Self::VERSION, self.setup).as_bytes()).to_string()
  }

  fn digest(&self, path: &std::path::Path) -> Option<Digest> {
    let mut digests = self.digests.lock().expect("not poisoned");
    *digests
      .entry(path.to_owned())
      .or_insert_with(|| Digest::of_file(path))
  }

  fn modified_since_start(&self, path: &std::path::Path) -> bool {
    std::fs::metadata(path)
      .and_then(|m| m.modified())
      .map_or(true, |modified| modified >= self.started)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn run(source: &std::path::Path) -> TidyRun {
    TidyRun {
      source: source.to_owned(),
      success: true,
      output: "ok".into(),
      cached: false,
    }
  }

  #[test]
  fn replays_until_an_input_changes() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("a.cpp");
    let header = dir.path().join("a.h");
    std::fs::write(&source, "int a;").unwrap();
    std::fs::write(&header, "int b;").unwrap();
    let started = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    let cache = TidyCache::new(dir.path().join("cache"), "setup".into(), started).unwrap();
    assert!(cache.lookup(&source, "cmd").is_none());
    cache.store(&run(&source), "cmd", &[source.clone(), header.clone()]);
    let hit = cache.lookup(&source, "cmd").unwrap();
    assert!(hit.cached && hit.success && hit.output == "ok");
    assert!(cache.lookup(&source, "other command").is_none());

    std::fs::write(&header, "int c;").unwrap();
    let fresh = TidyCache::new(dir.path().join("cache"), "setup".into(), started).unwrap();
    assert!(fresh.lookup(&source, "cmd").is_none());
    let other_setup = TidyCache::new(dir.path().join("cache"), "v2".into(), started).unwrap();
    assert!(other_setup.lookup(&source, "cmd").is_none());
  }

  #[test]
  fn files_edited_during_the_run_are_not_cached() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("a.cpp");
    std::fs::write(&source, "int a;").unwrap();
    let started = std::time::SystemTime::now() - std::time::Duration::from_secs(5);
    let cache = TidyCache::new(dir.path().join("cache"), "s".into(), started).unwrap();
    cache.store(&run(&source), "cmd", std::slice::from_ref(&source));
    assert!(cache.lookup(&source, "cmd").is_none());
  }

  #[test]
  fn prune_drops_entries_of_removed_sources() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.cpp"), dir.path().join("b.cpp"));
    for path in [&a, &b] {
      std::fs::write(path, "").unwrap();
    }
    let started = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    let cache = TidyCache::new(dir.path().join("cache"), "s".into(), started).unwrap();
    cache.store(&run(&a), "cmd", std::slice::from_ref(&a));
    cache.store(&run(&b), "cmd", std::slice::from_ref(&b));
    cache.prune([&a]);
    assert!(cache.entry_path(&a).is_file() && !cache.entry_path(&b).exists());
  }
}
