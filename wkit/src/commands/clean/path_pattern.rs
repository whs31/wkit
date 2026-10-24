mod segment;

use self::segment::Segment;

#[derive(Debug, Clone)]
pub struct PathPattern {
  raw: String,
  components: Vec<Segment>,
}

impl PathPattern {
  pub fn new(raw: &str) -> anyhow::Result<Self> {
    let invalid = |why: &str| anyhow::anyhow!("invalid clean path `{raw}`: {why}");
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.starts_with('/') || std::path::Path::new(trimmed).is_absolute() {
      return Err(invalid("it must be relative to the project root"));
    }
    let mut components = Vec::new();
    for part in trimmed.split('/') {
      match part {
        "" | "." => {}
        ".." => return Err(invalid("it must stay inside the project")),
        p if p.contains("**") => return Err(invalid("`**` is not supported")),
        p if p.contains(['*', '?', '[', '{']) => components.push(Segment::Glob(
          globset::GlobBuilder::new(p)
            .literal_separator(true)
            .build()
            .map_err(|e| invalid(&e.to_string()))?
            .compile_matcher(),
        )),
        p => components.push(Segment::Literal(p.to_owned())),
      }
    }
    if components.is_empty() {
      return Err(invalid("it would remove the whole project"));
    }
    Ok(Self {
      raw: raw.to_owned(),
      components,
    })
  }

  pub fn expand(&self, root: &std::path::Path) -> anyhow::Result<Vec<std::path::PathBuf>> {
    let mut frontier = vec![root.to_owned()];
    for segment in &self.components {
      let mut next = Vec::new();
      for dir in &frontier {
        match segment {
          Segment::Literal(name) => {
            let path = dir.join(name);
            if std::fs::symlink_metadata(&path).is_ok() {
              next.push(path);
            }
          }
          Segment::Glob(glob) => {
            let Ok(entries) = std::fs::read_dir(dir) else {
              continue;
            };
            for entry in entries {
              let entry = entry.map_err(|e| {
                anyhow::Error::from(e).context(format!("failed to expand `{}`", self.raw))
              })?;
              if glob.is_match(entry.file_name()) {
                next.push(entry.path());
              }
            }
          }
        }
      }
      frontier = next;
    }
    frontier.sort();
    Ok(frontier)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn expands_literals_and_globs_under_the_root() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for path in [
      "build/x",
      "cmake-build-debug/y",
      "cmake-build-release/z",
      "docs/graph.html",
    ] {
      std::fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
      std::fs::write(root.join(path), "").unwrap();
    }
    let expand = |p: &str| -> Vec<String> {
      PathPattern::new(p)
        .unwrap()
        .expand(root)
        .unwrap()
        .iter()
        .map(|p| {
          p.strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/")
        })
        .collect()
    };
    assert_eq!(expand("build/"), ["build"]);
    assert_eq!(
      expand("cmake-build-*"),
      ["cmake-build-debug", "cmake-build-release"]
    );
    assert_eq!(expand("docs/graph.html"), ["docs/graph.html"]);
    assert_eq!(expand("*/z"), ["cmake-build-release/z"]);
    assert!(expand("target").is_empty());
  }

  #[test]
  fn rejects_paths_outside_the_project() {
    for raw in ["/etc", "../x", "a/../../b", ".", "", "**/build"] {
      assert!(PathPattern::new(raw).is_err(), "{raw}");
    }
  }
}
