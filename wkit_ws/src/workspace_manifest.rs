use crate::{Error, Member, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceManifest {
  pub members: Vec<Member>,
}

impl WorkspaceManifest {
  pub const FILE: &str = "workspace.toml";
  pub const VERSION: i64 = 1;

  pub fn load(path: &std::path::Path) -> Result<Self> {
    Self::parse(&Self::read(path)?)
  }

  pub fn parse(text: &str) -> Result<Self> {
    let invalid = |message: String| Error::config(Self::FILE, message);
    let table: toml::Table =
      toml::from_str(text).map_err(|e| invalid(format!("invalid TOML: {e}")))?;
    if table.get("version").and_then(toml::Value::as_integer) != Some(Self::VERSION) {
      return Err(invalid("`version` must be 1".to_owned()));
    }
    let entries = table
      .get("project")
      .and_then(toml::Value::as_array)
      .filter(|entries| !entries.is_empty())
      .ok_or_else(|| invalid("expected one or more `[[project]]` entries".to_owned()))?;
    let mut members = Vec::with_capacity(entries.len());
    for entry in entries {
      let entry = entry
        .as_table()
        .ok_or_else(|| invalid("each project must be a table".to_owned()))?;
      let field = |key: &str| {
        entry
          .get(key)
          .and_then(toml::Value::as_str)
          .unwrap_or_default()
      };
      let path = field("path");
      if !Member::is_valid_path(path) {
        return Err(invalid(format!(
          "project path must be normalized and relative: `{path}`"
        )));
      }
      if field("url").is_empty() {
        return Err(invalid(format!("{path}: `url` must be a non-empty string")));
      }
      if !Member::is_valid_branch(field("branch")) {
        return Err(invalid(format!(
          "{path}: invalid branch `{}`",
          field("branch")
        )));
      }
      if members.iter().any(|m: &Member| m.path == path) {
        return Err(invalid(format!("duplicate project path: {path}")));
      }
      members.push(Member::new(path, field("url"), field("branch")));
    }
    let mut paths: Vec<_> = members.iter().map(|m| m.path.as_str()).collect();
    paths.sort_unstable();
    for pair in paths.windows(2) {
      if pair[1].starts_with(&format!("{}/", pair[0])) {
        return Err(invalid(format!(
          "overlapping project paths: {} and {}",
          pair[0], pair[1]
        )));
      }
    }
    Ok(Self { members })
  }

  pub fn member(&self, path: &str) -> Option<&Member> {
    self.members.iter().find(|m| m.path == path)
  }

  pub fn derive_path(&self, url: &str) -> String {
    let mut bases: Vec<(String, usize)> = Vec::new();
    for member in &self.members {
      let stem = member.url.trim_end_matches(".git");
      if let Some(base) = stem.strip_suffix(&member.path).filter(|b| b.ends_with('/')) {
        match bases.iter_mut().find(|(b, _)| b == base) {
          Some((_, count)) => *count += 1,
          None => bases.push((base.to_owned(), 1)),
        }
      }
    }
    bases.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let stem = url.trim_end_matches('/').trim_end_matches(".git");
    bases
      .iter()
      .find_map(|(base, _)| {
        stem
          .strip_prefix(base.as_str())
          .filter(|rest| !rest.is_empty())
      })
      .unwrap_or_else(|| stem.rsplit('/').next().unwrap_or(stem))
      .to_owned()
  }

  pub fn with_member(&self, text: &str, member: &Member) -> String {
    let parts: Vec<_> = member.path.split('/').collect();
    let shared = |other: &Member| {
      parts[..parts.len() - 1]
        .iter()
        .zip(other.path.split('/'))
        .take_while(|(a, b)| *a == b)
        .count()
    };
    let best = (0..self.members.len()).max_by_key(|&i| (shared(&self.members[i]), i));
    let mut lines: Vec<String> = text.split_inclusive('\n').map(ToOwned::to_owned).collect();
    let starts: Vec<_> = lines
      .iter()
      .enumerate()
      .filter(|(_, line)| line.trim() == "[[project]]")
      .map(|(i, _)| i)
      .collect();
    let position = match best {
      Some(best) if starts.len() == self.members.len() && shared(&self.members[best]) > 0 => {
        starts.get(best + 1).copied().unwrap_or(lines.len())
      }
      _ => lines.len(),
    };
    let mut entry = format!(
      "[[project]]\npath = {}\nurl = {}\nbranch = {}\n",
      toml::Value::from(member.path.as_str()),
      toml::Value::from(member.url.as_str()),
      toml::Value::from(member.branch.as_str()),
    );
    if position < lines.len() {
      entry.push('\n');
    } else {
      if let Some(last) = lines.last_mut().filter(|l| !l.ends_with('\n')) {
        last.push('\n');
      }
      if lines.last().is_some_and(|l| !l.trim().is_empty()) {
        entry.insert(0, '\n');
      }
    }
    lines.insert(position, entry);
    lines.concat()
  }

  pub(crate) fn read(path: &std::path::Path) -> Result<String> {
    let name = path
      .file_name()
      .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    match std::fs::read_to_string(path) {
      Ok(text) => Ok(text),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Error::config(name, "missing")),
      Err(source) => Err(
        wkit_core::Error::Read {
          path: path.to_owned(),
          source,
        }
        .into(),
      ),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const MANIFEST: &str = r#"# Ordered project catalog.
version = 1

[[project]]
path = "support/rcommon"
url = "http://git.example/dev/support/rcommon.git"
branch = "main"

[[project]]
path = "protocol/xlink"
url = "http://git.example/dev/protocol/xlink.git"
branch = "main"
"#;

  fn problem(text: &str) -> String {
    WorkspaceManifest::parse(text).unwrap_err().to_string()
  }

  #[test]
  fn reads_the_ordered_catalog() {
    let manifest = WorkspaceManifest::parse(MANIFEST).unwrap();
    let paths: Vec<_> = manifest.members.iter().map(|m| m.path.as_str()).collect();
    assert_eq!(paths, ["support/rcommon", "protocol/xlink"]);
    assert_eq!(manifest.member("protocol/xlink").unwrap().branch, "main");
  }

  #[test]
  fn rejects_what_the_python_tooling_rejects() {
    assert!(
      problem("version = 2\n[[project]]\npath='a'\nurl='u'\nbranch='m'\n")
        .contains("`version` must be 1")
    );
    assert!(problem("version = 1\n").contains("one or more"));
    assert!(
      problem("version = 1\n[[project]]\npath='../a'\nurl='u'\nbranch='m'\n")
        .contains("normalized")
    );
    assert!(problem("version = 1\n[[project]]\npath='a'\nurl=''\nbranch='m'\n").contains("`url`"));
    assert!(
      problem("version = 1\n[[project]]\npath='a'\nurl='u'\nbranch='-x'\n")
        .contains("invalid branch")
    );
    let twice = "version = 1\n[[project]]\npath='a'\nurl='u'\nbranch='m'\n[[project]]\npath='a'\nurl='v'\nbranch='m'\n";
    assert!(problem(twice).contains("duplicate project path: a"));
    let nested = "version = 1\n[[project]]\npath='a'\nurl='u'\nbranch='m'\n[[project]]\npath='a/b'\nurl='v'\nbranch='m'\n";
    assert!(problem(nested).contains("overlapping project paths: a and a/b"));
    let siblings = "version = 1\n[[project]]\npath='a'\nurl='u'\nbranch='m'\n[[project]]\npath='ab'\nurl='v'\nbranch='m'\n";
    assert!(WorkspaceManifest::parse(siblings).is_ok());
  }

  #[test]
  fn new_paths_follow_the_shared_url_layout() {
    let manifest = WorkspaceManifest::parse(MANIFEST).unwrap();
    assert_eq!(
      manifest.derive_path("http://git.example/dev/protocol/xschema.git"),
      "protocol/xschema"
    );
    assert_eq!(
      manifest.derive_path("https://github.com/x/tool.git/"),
      "tool"
    );
  }

  #[test]
  fn new_members_land_next_to_their_group() {
    let manifest = WorkspaceManifest::parse(MANIFEST).unwrap();
    let member = Member::new(
      "support/gatekeep",
      "http://git.example/dev/support/gatekeep.git",
      "main",
    );
    let text = manifest.with_member(MANIFEST, &member);
    let added = WorkspaceManifest::parse(&text).unwrap();
    let paths: Vec<_> = added.members.iter().map(|m| m.path.as_str()).collect();
    assert_eq!(
      paths,
      ["support/rcommon", "support/gatekeep", "protocol/xlink"]
    );
    assert!(
      text.contains("branch = \"main\"\n\n[[project]]\npath = \"protocol/xlink\""),
      "{text}"
    );

    let other = Member::new("tools/x", "u", "dev");
    let text = manifest.with_member(MANIFEST.trim_end(), &other);
    assert!(
      text.ends_with(
        "branch = \"main\"\n\n[[project]]\npath = \"tools/x\"\nurl = \"u\"\nbranch = \"dev\"\n"
      ),
      "{text}"
    );
  }
}
